//! Source operations lowered by upstream retain their C-observable behavior.
#[path = "compat/reused/support.rs"]
#[allow(dead_code)]
mod support;

use emelino_cli::{config::Config, frontend, upstream_adapter};
use emelino_compiler::ir::{Builtin, RuntimeOp, Stmt, Term, Value};
use std::path::Path;

fn run(name: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compat/builtins")
        .join(name);
    support::run_fixture_at(name, &fixture, false);
}

#[test]
fn debug_fallback_preserves_custom_show_and_once_only_evaluation() {
    run("debug");
}
#[test]
fn panic_todo_and_assert_stop_at_the_board_hook() {
    run("defects");
}
#[test]
fn inspect_and_nested_string_show_preserve_escaping() {
    run("display");
}

#[test]
fn source_operations_are_lowered_by_upstream_before_the_backend_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compat/builtins");
    let source_only = [
        Builtin::AssertFail,
        Builtin::Dbg,
        Builtin::DebugPrint,
        Builtin::Inspect,
        Builtin::Panic,
        Builtin::ShowNested,
        Builtin::ShowString,
        Builtin::Todo,
    ];
    let (mut show, mut debug, mut defect) = (false, false, false);
    for name in ["debug", "defects", "display"] {
        let fixture = root.join(name);
        let source = std::fs::read_to_string(fixture.join("main.emel")).unwrap();
        let config =
            Config::from_json(&std::fs::read_to_string(fixture.join("config.json")).unwrap())
                .unwrap();
        let generated = frontend::generate(&source, Path::new("main.emel")).unwrap();
        let program =
            upstream_adapter::adapt(&generated.module, &config.export_pairs().unwrap()).unwrap();
        for (_, function) in program.functions.iter() {
            for (_, statement) in function.stmts.iter() {
                if let Stmt::Let { value, .. } = statement {
                    match value {
                        Value::Builtin { op, .. } => {
                            assert!(!source_only.contains(op), "residual {op:?}")
                        }
                        Value::Runtime {
                            op: RuntimeOp::ShowString,
                            ..
                        } => show = true,
                        Value::Runtime {
                            op: RuntimeOp::DebugPrint,
                            ..
                        } => debug = true,
                        _ => {}
                    }
                }
            }
            defect |= function
                .seqs
                .iter()
                .any(|(_, seq)| matches!(seq.term, Term::Defect(_)));
        }
        emelino_cli::compile(&source, &config).unwrap();
    }
    assert!(show && debug && defect);
}
