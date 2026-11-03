use super::*;
use std::path::Path;

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
