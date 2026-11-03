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
fn const_specializations_keep_upstream_identity() {
    let program = source_program("const VALUE = 42\npub fn answer() -> Int { VALUE }");
    assert!(program.const_function_groups.is_empty());
    assert!(program.functions.iter().any(|(_, f)| f.constant));
    assert!(
        program
            .functions
            .iter()
            .all(|(_, f)| f.canonical_const.is_none())
    );
}
