#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn embedded_handlers_preserve_services_buffers_errors_and_cancellation() {
    support::run_fixture("p7-embedded", true);
}

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
fn upstream_standard_namespace_and_reserved_compatibility_namespace_are_distinct() {
    // The adopted upstream std owns Std.Embedded.Gpio. The independent
    // frontend's old expectation that this namespace is absent is superseded.
    emelino_cli::frontend::generate(
        "import Std.Embedded.Gpio.{Gpio}\nfn main() { 0 }",
        std::path::Path::new("main.emel"),
    )
    .unwrap();
    emelino_cli::frontend::generate(
        "import Std.Embedded.Gpio.{Gpio}\npub fn answer()->Int use Gpio {42}",
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

#[test]
fn fixed_board_and_unrelated_embedded_sources_preserve_artifact_expectations() {
    use emelino_cli::{
        frontend::{Source, generate_sources},
        upstream_adapter,
    };
    use emelino_compiler::{BackendInput, Options};
    let main = include_str!(
        "compat/legacy-corpus/sources/a009a39326fe15f46d2ba09f8a64c849be3dd60e11ac363a3a67e008f2f39d0b.emel"
    );
    let flow = generate_sources(
        &[Source {
            name: "main.emel",
            text: main,
        }],
        "main.emel",
        std::path::Path::new("app/src/main.emel"),
    )
    .unwrap();
    let program =
        upstream_adapter::adapt(&flow.module, &[("main".into(), "board_main".into())]).unwrap();
    let input = BackendInput::new(program).unwrap();
    let options = Options::default()
        .with_entry("main")
        .with_heap_bytes(4096)
        .with_frame_bytes(2048);
    let output = input.compile(&options).unwrap();
    assert_eq!(
        output,
        BackendInput::from_json(&input.to_json().unwrap())
            .unwrap()
            .compile(&options)
            .unwrap()
    );
    for name in ["emela_clock.h", "emela_console.h"] {
        assert!(output.files.iter().any(|file| file.path == name));
    }
    let sources = [
        Source {
            name: "main.emel",
            text: include_str!(
                "compat/legacy-corpus/sources/bc4bbf80ab40493a7f88d12b34fada853e417876816258f7f9f74115830d2b78.emel"
            ),
        },
        Source {
            name: "embedded/local.emel",
            text: include_str!(
                "compat/legacy-corpus/sources/707d8cd11e9ffe7d42d9dbd50b2a66fc1a656b0e05f97e6ae9c19fea71638695.emel"
            ),
        },
    ];
    let flow = generate_sources(
        &sources,
        "main.emel",
        std::path::Path::new("app/src/main.emel"),
    )
    .unwrap();
    let program =
        upstream_adapter::adapt(&flow.module, &[("main".into(), "local_main".into())]).unwrap();
    emelino_compiler::compile_program(program, &Options::default()).unwrap();
}
