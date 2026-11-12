//! Allocation elimination and snapshot semantics through source and saved IR.
#[path = "support/heap.rs"]
mod heap;
mod support;

use emelino_compiler::{BackendInput, Options, ir::*};
use std::path::Path;

const DEFECT: &str = r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  (void)k; (void)m; (void)n; abort();
}
"#;

#[test]
fn local_scalar_closures_and_factories_run_without_a_heap() {
    let source = format!(
        "{}\n{}",
        include_str!("../../../benchmarks/runtime/cases/closure.emel"),
        r#"
fn pair(a: Int, b: Int) -> fn(Int) -> Int { |x| a * 10 + b + x }
pub fn two(n: Int) -> Int {
  first = pair(n, 2)
  second = pair(n + 1, 3)
  first(4) * 100 + second(5)
}
pub fn nested(n: Int) -> Int {
  f = |x: Int| { g = |y: Int| n + x + y
    g(3) }
  f(4)
}
pub fn scalars(i: Int, wide: Int64, real: Float, enabled: Bool) -> Bool {
  f = |value: Int| enabled && value == i && wide == Int64.from_int(i) && real == 2.5
  f(i) && f(i)
}
"#
    );
    let output = support::compile(
        &source,
        &[
            ("bench", "bench"),
            ("two", "two"),
            ("nested", "nested"),
            ("scalars", "scalars"),
        ],
        &Options::default().with_heap_bytes(0),
    );
    assert!(!output.c_source.contains("emela_heap_allocate("));
    support::compile_and_run(
        &output,
        &format!(
            "{DEFECT}\n{}",
            r#"
int main(void) {
  for (int n = 0; n < 100; ++n) {
    if (bench(n, 17) != 17 + n * (n + 1) / 2) return 1;
    if (two(n) != (n * 10 + 6) * 100 + (n + 1) * 10 + 8) return 2;
    if (nested(n) != n + 7) return 3;
    if (!scalars(n, (int64_t)n, 2.5, 1)) return 4;
    if (scalars(n, (int64_t)n, 2.5, 0)) return 5;
    if (scalars(n, (int64_t)n + 1, 2.5, 1)) return 6;
  }
  return bench(10000, 17) != 50005017;
}
"#
        ),
    );
}

#[test]
fn captures_are_snapshots_even_when_saved_ir_redefines_the_source() {
    let generated = emelino_cli::frontend::generate(
        "pub fn answer(n: Int) -> Int { f = |x: Int| n + x\n f(n) }",
        Path::new("main.emel"),
    )
    .unwrap();
    let mut program = emelino_cli::upstream_adapter::adapt(
        &generated.module,
        &[("answer".into(), "answer".into())],
    )
    .unwrap();
    let (_, function) = program
        .functions
        .iter_mut()
        .find(|(_, f)| f.exported)
        .unwrap();
    let n = function.params[0];
    let call = function
        .stmts
        .iter()
        .find_map(|(id, statement)| {
            matches!(
                statement,
                Stmt::Let {
                    value: Value::Call {
                        callee: Callee::Indirect(_),
                        ..
                    },
                    ..
                }
            )
            .then_some(id)
        })
        .unwrap();
    let update = function.stmts.alloc(Stmt::Let {
        dst: Some(n),
        value: Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![Operand::Var(n), Operand::Const(Const::Int(1))],
        },
        on_defect: None,
        on_suspend: None,
    });
    for (_, seq) in function.seqs.iter_mut() {
        if let Some(index) = seq.stmts.iter().position(|&id| id == call) {
            seq.stmts.insert(index, update);
        }
    }
    let input = BackendInput::new(program).unwrap();
    let options = Options::default().with_heap_bytes(0);
    let output = input.compile(&options).unwrap();
    assert_eq!(
        output,
        BackendInput::from_json(&input.to_json().unwrap())
            .unwrap()
            .compile(&options)
            .unwrap()
    );
    support::compile_and_run(
        &output,
        &format!(
            "{DEFECT}\n{}",
            r#"
int main(void) {
  for (int n = -100; n < 100; ++n) if (answer(n) != n * 2 + 1) return 1;
  return 0;
}
"#
        ),
    );
}

#[test]
fn escaping_and_managed_closures_keep_owned_environments() {
    let source = r#"
fn adder(n: Int) -> fn(Int) -> Int { |x| n + x }
fn keep(text: String) -> fn() -> String { || text }
pub fn answer(n: Int) -> Bool {
  callbacks = [adder(n), adder(n + 1)]
  value = List.fold(callbacks, 0, |sum, f| sum + f(10))
  text = "value#{n}"
  f = keep(text)
  value == n * 2 + 21 && f() == text
}
"#;
    let options = Options::default().with_heap_bytes(2048);
    let output = heap::with_heap_audit(support::compile(source, &[("answer", "answer")], &options));
    support::compile_and_run(
        &output,
        &format!(
            "{DEFECT}\n{}",
            r#"
int test_heap_empty(void);
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    if (!answer(i) || !test_heap_empty()) return 1;
  }
  return 0;
}
"#
        ),
    );
}

#[test]
fn failing_local_closures_preserve_failure_and_cleanup() {
    let source = r#"
error Oops(value: Int)
fn failer(n: Int) -> fn() -> Int fails Oops { || fail Oops(n) }
pub fn answer(n: Int) -> Int {
  f = failer(n)
  f() escape { Oops(value) -> value + 1 }
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("answer", "answer")],
        &Options::default().with_heap_bytes(1024),
    ));
    support::compile_and_run(
        &output,
        &format!(
            "{DEFECT}\n{}",
            r#"
int test_heap_empty(void);
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    if (answer(i) != i + 1 || !test_heap_empty()) return 1;
  }
  return 0;
}
"#
        ),
    );
}
