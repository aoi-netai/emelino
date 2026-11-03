#[test]
fn public_entries_require_handled_effects_instead_of_exposing_evidence_pointers() {
    let config = emelino_cli::config::Config::from_json(
        r#"{"exports":[{"function":"entry","symbol":"entry"}],"heap_bytes":4096}"#,
    )
    .unwrap();
    let source = "effect Counter { fn read() -> Int }\npub fn entry() -> Int use Counter { (use Counter).read() }";
    let error = emelino_cli::compile(source, &config).unwrap_err();
    assert!(error.contains("unresolved effect evidence"), "{error}");
}
