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
fn extension_modules_cannot_be_replaced_by_user_sources() {
    let sources = [
        emelino_cli::frontend::Source {
            name: "main.emel",
            text: "pub fn answer()->Int {42}",
        },
        emelino_cli::frontend::Source {
            name: "embedded/board.emel",
            text: "pub fn forged()->Int {1}",
        },
    ];
    let error = emelino_cli::frontend::generate_sources(
        &sources,
        "main.emel",
        std::path::Path::new("main.emel"),
    )
    .err()
    .unwrap();
    assert!(
        error.contains("conflicts with an emelino extension"),
        "{error}"
    );
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

#[test]
fn embedded_namespace_is_owned_and_std_embedded_is_not_supplied() {
    // Embedded modules belong to emelino. Even while the pinned upstream still
    // bundles Std.Embedded, it must not supply a second copy of these contracts.
    for source in [
        "import Std.Embedded.Gpio.{Gpio}\nfn main() { 0 }",
        "import Std.Embedded.Gpio.{Gpio}\npub fn answer()->Int use Gpio {42}",
    ] {
        let error = emelino_cli::frontend::generate(source, std::path::Path::new("main.emel"))
            .err()
            .expect("upstream Std.Embedded modules must not be supplied");
        assert!(error.contains("Std.Embedded.Gpio"), "{error}");
    }
    emelino_cli::frontend::generate(
        "import Embedded.Gpio.{Gpio}\npub fn answer()->Int use Gpio {42}",
        std::path::Path::new("main.emel"),
    )
    .unwrap();
    let inputs = [
        emelino_cli::frontend::Source {
            name: "main.emel",
            text: "pub fn answer()->Int {42}",
        },
        emelino_cli::frontend::Source {
            name: "embedded/gpio.emel",
            text: "pub fn fake()->Int {99}",
        },
    ];
    let error = emelino_cli::frontend::generate_sources(
        &inputs,
        "main.emel",
        std::path::Path::new("main.emel"),
    )
    .err()
    .unwrap();
    assert!(
        error.contains("conflicts with an emelino extension"),
        "{error}"
    );
}
