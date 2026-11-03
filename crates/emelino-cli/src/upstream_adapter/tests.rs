use super::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn source_program(source: &str) -> d::Program {
    let generated = crate::frontend::generate(source, Path::new("main.emel")).unwrap();
    adapt(&generated.module, &[("answer".into(), "answer".into())]).unwrap()
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "emelino-adapter-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn execute(source: &str, invocation: &str) {
    let program = source_program(source);
    let generated = emelino_compiler::compile_program(
        program,
        &emelino_compiler::Options::default().with_heap_bytes(16384),
    )
    .unwrap_or_else(|errors| panic!("{source}\n{errors:#?}"));
    let scratch = Scratch::new();
    for file in generated.files {
        let path = scratch.0.join(file.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, file.contents).unwrap();
    }
    let harness = format!(
        r#"
#include "emela_embedded.h"
#include "emela_board.h"
#include <stdlib.h>
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {{
    (void)kind; (void)message; (void)length; abort();
}}
int main(void) {{ return {invocation} == 42 ? 0 : 1; }}
"#
    );
    std::fs::write(scratch.0.join("host.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let binary = scratch.0.join(format!("host{}", &optimization[2..]));
        let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .current_dir(&scratch.0)
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
                optimization,
                "emela_embedded.c",
                "host.c",
                "-lm",
                "-o",
            ])
            .arg(&binary)
            .output()
            .expect("a C compiler is required");
        assert!(
            output.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(binary).output().unwrap();
        assert!(
            output.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn upstream_calls_branches_and_composite_type_ids_execute() {
    for (source, invocation) in [
        (
            "fn twice(n: Int) -> Int { n * 2 }\npub fn answer(n: Int) -> Int { if n > 0 { twice(n) + 32 } else { -1 } }",
            "answer(5)",
        ),
        (
            "pub fn answer(n: Int) -> Int { (a, b) = (n, 37)\na + b }",
            "answer(5)",
        ),
        (
            "pub fn answer(n: Int) -> Int { List.length([n, 1, 2]) + 39 }",
            "answer(5)",
        ),
        (
            "type Row(value: Int, text: String)\npub fn answer(n: Int) -> Int { row = Row(value: n, text: \"ok\")\nrow.value + String.length(row.text) + 35 }",
            "answer(5)",
        ),
    ] {
        execute(source, invocation);
    }
}

#[test]
fn upstream_closure_error_and_handler_execute() {
    for (source, invocation) in [
        (
            "pub fn answer(n: Int) -> Int { f = fn(x: Int) { x + n }\nf(37) }",
            "answer(5)",
        ),
        (
            "error Gone(value: Int)\nfn fail_value(n: Int) -> Int fails Gone { fail Gone(value: n) }\npub fn answer(n: Int) -> Int { fail_value(n) escape { Gone(value:) -> value + 37 } }",
            "answer(5)",
        ),
        (
            "effect Read { fn read() -> Int }\nhandler Reader(value: Int): Read { init { Reader(value: 42) }\nfn read() { self.value } }\npub fn answer() -> Int { with Reader { (use Read).read() } }",
            "answer()",
        ),
    ] {
        execute(source, invocation);
    }
}

#[test]
fn configured_roots_do_not_expose_private_or_unselected_functions() {
    let generated = crate::frontend::generate(
        "fn helper() -> Int { 42 }\npub fn answer() -> Int { helper() }\npub fn other() -> Int { 1 }",
        Path::new("main.emel"),
    )
    .unwrap();
    let program = adapt(&generated.module, &[("answer".into(), "chosen".into())]).unwrap();
    let public: Vec<_> = program
        .functions
        .iter()
        .filter(|(_, f)| f.exported)
        .map(|(_, f)| f.export_symbol.as_deref())
        .collect();
    assert_eq!(public, vec![Some("chosen")]);
    assert!(
        adapt(&generated.module, &[("helper".into(), "helper".into())])
            .unwrap_err()
            .contains("public root")
    );
    assert!(
        adapt(&generated.module, &[("missing".into(), "missing".into())])
            .unwrap_err()
            .contains("absent")
    );
    assert!(
        adapt(
            &generated.module,
            &[
                ("answer".into(), "first".into()),
                ("answer".into(), "second".into())
            ]
        )
        .unwrap_err()
        .contains("duplicate configured function")
    );
    assert!(
        adapt(
            &generated.module,
            &[
                ("answer".into(), "same".into()),
                ("other".into(), "same".into())
            ]
        )
        .unwrap_err()
        .contains("duplicate configured C name")
    );
}

#[test]
fn later_ir_stages_are_rejected_before_backend_lowering() {
    let mut generated =
        crate::frontend::generate("pub fn answer() -> Int { 42 }", Path::new("main.emel")).unwrap();
    generated.module.stage = emela_lir::Stage::ClosuresConverted;
    assert!(
        adapt(&generated.module, &[("answer".into(), "answer".into())])
            .unwrap_err()
            .contains("raw Stage::Flow")
    );
}

#[test]
fn standard_bytes_preserve_canonical_ids_and_project_intrinsics() {
    let generated = crate::frontend::generate(
        "import Std.Bytes\npub fn answer() -> Int { Bytes.size(Bytes.encode_utf8(\"abc\")) + 39 }",
        Path::new("main.emel"),
    )
    .unwrap();
    let program = adapt(&generated.module, &[("answer".into(), "answer".into())]).unwrap();
    assert_eq!(type_id(emela_core::TypeId::BYTES), d::TypeId::BYTES);
    assert_eq!(program.core.ty(d::TypeId::BYTES), &d::Type::Bytes);
    for (index, source_type) in generated.module.core.types.iter().enumerate() {
        assert_eq!(
            program.core.ty(d::TypeId::from_index(index)),
            &adapt_type(source_type),
        );
    }
    for expected in [d::Builtin::BytesEncodeUtf8, d::Builtin::BytesSize] {
        assert!(program.functions.iter().any(|(_, function)| {
            function.stmts.iter().any(|(_, statement)| {
                matches!(statement, d::Stmt::Let { value: d::Value::Builtin { op, .. }, .. } if *op == expected)
            })
        }));
    }
    assert!(
        program
            .functions
            .iter()
            .all(|(_, function)| function.external.is_none())
    );
}

#[test]
fn user_bytes_name_keeps_its_nominal_type() {
    let program =
        source_program("type Bytes(value: Int)\npub fn answer() -> Int { Bytes(value: 42).value }");
    let (local, _) = program
        .core
        .enums
        .iter()
        .find(|(_, definition)| definition.name == "Bytes")
        .unwrap();
    assert!(
        program
            .core
            .types
            .types
            .iter()
            .any(|ty| matches!(ty, d::Type::Enum(id, _) if *id == local))
    );
    assert_eq!(program.core.ty(d::TypeId::BYTES), &d::Type::Bytes);
}

#[test]
fn erased_unit_callback_uses_a_unit_box_without_changing_empty_strings() {
    let program = source_program(
        "import Std.Fiber\npub fn answer() -> Int { Fiber.scope(fn() { child = Fiber.fork(fn() { () })\nFiber.join(child)\n42 }) }",
    );
    assert!(program.functions.iter().any(|(_, function)| function.stmts.iter().any(|(_, statement)| {
        matches!(statement, d::Stmt::Let { dst: Some(dst), value: d::Value::Coerce(source), .. }
            if function.vars[*dst].ty == d::TypeId::ANY && function.vars[*source].ty == d::TypeId::UNIT)
    })));
    let program = source_program("pub fn answer() -> String { \"\" }");
    assert!(program.functions.iter().any(|(_, function)| function.seqs.iter().any(|(_, sequence)| {
        matches!(&sequence.term, d::Term::Return(Some(d::Operand::Const(d::Const::String(value)))) if value.is_empty())
    })));
    assert!(!program.functions.iter().any(|(_, function)| function.stmts.iter().any(|(_, statement)| {
        matches!(statement, d::Stmt::Let { value: d::Value::Coerce(source), .. } if function.vars[*source].ty == d::TypeId::UNIT)
    })));
}
