#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn isr_limits_apply_after_upstream_generation() {
    let cases = [
        (
            "fn helper(value:Int)->Int { if value==0 {0} else {1+helper(value-1)} }\npub fn entry(value:Int)->Int {helper(value)}",
            "recursive call",
        ),
        (
            "pub fn entry(value:Int)->List[Int] {[value]}",
            "heap-backed",
        ),
        (
            "@external(c,\"board_seed\")\nfn seed()->Int use {}\nconst VALUE=seed()\npub fn entry()->Int {VALUE}",
            "runtime const initialization",
        ),
    ];
    for (source, expected) in cases {
        let mut config = emelino_cli::config::Config::from_json(r#"{"exports":[{"function":"entry","symbol":"run"}],"heap_bytes":8192,"isr_safe_externals":["board_seed"]}"#).unwrap();
        emelino_cli::compile(source, &config).unwrap();
        config.isr_entries.push("entry".into());
        let error = emelino_cli::compile(source, &config).unwrap_err();
        assert!(
            error.contains(expected) && error.contains("ISR entry"),
            "{error}"
        );
    }
}

#[test]
fn isr_adapters_preserve_integer_float_bits_and_static_borrows() {
    support::run_fixture("p7-isr-adapters", false);
    support::run_fixture("p7-isr-static", false);
}

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
