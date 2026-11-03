use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "emelino-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn cli(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_emelino"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

fn run_c(directory: &Path, output: &str, harness: &str) {
    std::fs::write(directory.join("host.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.join(format!("host{}", &optimization[2..]));
        success(
            Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .current_dir(directory)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic"])
                .arg(optimization)
                .arg("-I")
                .arg(output)
                .arg(format!("{output}/emela_embedded.c"))
                .arg("host.c")
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("a C compiler is required for the pipeline test"),
        );
        success(Command::new(executable).output().unwrap());
    }
}

fn config(directory: &Path, function: &str, symbol: &str) {
    std::fs::write(
        directory.join("config.json"),
        serde_json::json!({
            "exports": [{"function": function, "symbol": symbol}]
        })
        .to_string(),
    )
    .unwrap();
}

fn host(body: &str) -> String {
    format!(
        "#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n#include <stdlib.h>\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {{ (void)k;(void)m;(void)n;abort(); }}\nint main(void) {{ {body} }}\n"
    )
}

#[test]
fn upstream_flow_survives_source_removal_and_links_from_c() {
    let scratch = Scratch::new();
    std::fs::write(
        scratch.0.join("answer.emel"),
        include_str!("../../../examples/answer.emel"),
    )
    .unwrap();
    config(&scratch.0, "answer", "answer");
    success(cli(
        &scratch.0,
        &[
            "flow",
            "answer.emel",
            "--config",
            "config.json",
            "--out-dir",
            "flow",
        ],
    ));
    let dump = std::fs::read_to_string(scratch.0.join("flow/program.flow.txt")).unwrap();
    assert!(dump.contains("42"), "{dump}");
    assert!(!scratch.0.join("flow/emela_embedded.c").exists());
    std::fs::remove_file(scratch.0.join("answer.emel")).unwrap();
    std::fs::remove_file(scratch.0.join("config.json")).unwrap();
    success(cli(
        &scratch.0,
        &[
            "build",
            "--input",
            "flow/program.backend.json",
            "--config",
            "flow/program.config.json",
            "--out-dir",
            "c",
        ],
    ));
    run_c(&scratch.0, "c", include_str!("../../../examples/host.c"));
}

#[test]
fn source_build_preserves_alias_arguments_calls_and_branches() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("different.emel"), "fn helper(value: Int) -> Int { value + 2 }\npub fn answer(value: Int) -> Int { if value > 0 { helper(value) } else { 17 } }\n").unwrap();
    config(&scratch.0, "answer", "read_value");
    success(cli(
        &scratch.0,
        &[
            "build",
            "different.emel",
            "--config",
            "config.json",
            "--out-dir",
            "out",
        ],
    ));
    assert!(scratch.0.join("out/program.backend.json").is_file());
    run_c(
        &scratch.0,
        "out",
        &host("return read_value(40) == 42 && read_value(0) == 17 ? 0 : 1;"),
    );
}

#[test]
fn source_build_resolves_explicit_modules() {
    let scratch = Scratch::new();
    std::fs::write(
        scratch.0.join("main.emel"),
        "import Math\npub fn answer() -> Int { Math.add(40, 2) }",
    )
    .unwrap();
    std::fs::write(
        scratch.0.join("math.emel"),
        "pub fn add(a: Int, b: Int) -> Int { a + b }",
    )
    .unwrap();
    std::fs::write(
        scratch.0.join("config.json"),
        r#"{"exports":[{"function":"answer","symbol":"answer"}],"sources":["math.emel"]}"#,
    )
    .unwrap();
    success(cli(
        &scratch.0,
        &[
            "build",
            "main.emel",
            "--config",
            "config.json",
            "--out-dir",
            "out",
        ],
    ));
    run_c(&scratch.0, "out", &host("return answer() == 42 ? 0 : 1;"));
}

#[test]
fn frontend_errors_and_invalid_entry_contracts_do_not_emit_c() {
    let scratch = Scratch::new();
    config(&scratch.0, "answer", "answer");
    let inputs = [
        "pub fn answer() -> Int {",
        "pub fn answer() -> Int { \"not an Int\" }",
        "fn answer() -> Int { 42 }",
        "@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
    ];
    for (index, source) in inputs.iter().enumerate() {
        std::fs::write(scratch.0.join("input.emel"), source).unwrap();
        let directory = format!("case{index}");
        let result = cli(
            &scratch.0,
            &[
                "build",
                "input.emel",
                "--config",
                "config.json",
                "--out-dir",
                &directory,
            ],
        );
        assert!(!result.status.success(), "unexpectedly accepted {source}");
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.starts_with("emelino: "), "{error}");
        assert!(!error.contains("panicked"), "{error}");
        assert!(!scratch.0.join(&directory).exists());
    }
}

#[test]
fn invalid_saved_input_config_and_arguments_fail_without_outputs() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("bad.json"), "{not json}").unwrap();
    std::fs::write(scratch.0.join("replay.json"), "{}").unwrap();
    for args in [
        vec![
            "build",
            "--input",
            "bad.json",
            "--config",
            "replay.json",
            "--out-dir",
            "out",
        ],
        vec!["build", "--out-dir", "out"],
        vec!["flow", "--input", "bad.json", "--out-dir", "out"],
        vec!["build", "a.emel", "--input", "bad.json", "--out-dir", "out"],
        vec!["build", "a.emel", "--out-dir", "--input"],
        vec!["build", "a.emel", "--out-dir", "out"],
    ] {
        let result = cli(&scratch.0, &args);
        assert!(!result.status.success(), "{args:?}");
        assert!(!scratch.0.join("out").exists());
    }
}

#[test]
fn invalid_saved_function_sets_are_diagnosed_without_outputs() {
    use serde_json::json;

    let generated =
        emelino_cli::frontend::generate("pub fn answer() -> Int { 42 }", Path::new("main.emel"))
            .unwrap();
    let program = emelino_cli::upstream_adapter::adapt(
        &generated.module,
        &[("answer".into(), "answer".into())],
    )
    .unwrap();
    let saved = emelino_compiler::BackendInput::new(program)
        .unwrap()
        .to_json()
        .unwrap();
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("config.json"), "{}").unwrap();
    for (field, kind) in [("errors", "error"), ("effects", "effect")] {
        let mut input: serde_json::Value = serde_json::from_str(&saved).unwrap();
        let types = input["program"]["core"]["types"]["types"]
            .as_array_mut()
            .unwrap();
        let empty_set = types.len();
        types.push(json!({"Set": {
            "fails": false, "suspends": false, "ev": false, "params": []
        }}));
        let callback = types.len();
        let mut function = json!({"Fn": {
            "params": [], "ret": 0,
            "fails": false, "suspends": false, "ev": false,
            "sets": {"errors": empty_set, "effects": empty_set}
        }});
        function["Fn"]["sets"][field] = json!(0); // Int is not a set.
        types.push(function);
        let wrapper_type = types.len();
        let wrapper = input["program"]["core"]["enums"].as_array().unwrap().len();
        input["program"]["core"]["enums"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "name": "Callback", "params": 0,
                "variants": [{
                    "name": "Callback", "fields": {"Positional": 1}, "tys": [callback]
                }],
                "error": false, "external": false
            }));
        input["program"]["core"]["types"]["types"]
            .as_array_mut()
            .unwrap()
            .push(json!({"Enum": [wrapper, []]}));
        input["program"]["functions"][0]["vars"]
            .as_array_mut()
            .unwrap()
            .push(json!({"ty": wrapper_type, "repr": "Ref", "hint": "unused_callback"}));
        std::fs::write(scratch.0.join("input.json"), input.to_string()).unwrap();
        let result = cli(
            &scratch.0,
            &[
                "build",
                "--input",
                "input.json",
                "--config",
                "config.json",
                "--out-dir",
                field,
            ],
        );
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert_eq!(result.status.code(), Some(1), "{diagnostic}");
        assert!(
            diagnostic.contains(&format!(
                "backend input: function {kind} set must reference a set type"
            )),
            "{diagnostic}"
        );
        assert!(!diagnostic.contains("panicked"), "{diagnostic}");
        assert!(!scratch.0.join(field).exists());
    }
}

#[test]
fn unknown_config_and_duplicate_symbols_are_diagnosed() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("main.emel"), "pub fn answer() -> Int { 42 }").unwrap();
    for json in [
        r#"{"exports":[{"function":"answer","symbol":"answer"}],"heep_bytes":1024}"#,
        r#"{"exports":[{"function":"answer","symbol":"answer"},{"function":"answer","symbol":"answer"}]}"#,
        r#"{"exports":[{"function":"answer","symbol":"answer"}],"entry":"missing"}"#,
    ] {
        std::fs::write(scratch.0.join("config.json"), json).unwrap();
        let result = cli(
            &scratch.0,
            &[
                "build",
                "main.emel",
                "--config",
                "config.json",
                "--out-dir",
                "out",
            ],
        );
        assert!(!result.status.success(), "{json}");
        assert!(!scratch.0.join("out").exists());
    }
}

#[test]
fn saved_raw_evidence_input_cannot_bypass_the_cli_entry_contract() {
    use emelino_compiler::ir::{Repr, TypeId};
    let generated = emelino_cli::frontend::generate(
        "pub fn passthrough(value: Int) -> Int { value }",
        Path::new("main.emel"),
    )
    .unwrap();
    let mut program = emelino_cli::upstream_adapter::adapt(
        &generated.module,
        &[("passthrough".into(), "passthrough".into())],
    )
    .unwrap();
    let function = program
        .functions
        .iter_mut()
        .find(|(_, f)| f.exported)
        .unwrap()
        .1;
    function.ret = TypeId::EVIDENCE;
    for (_, variable) in function.vars.iter_mut() {
        variable.ty = TypeId::EVIDENCE;
        variable.repr = Repr::Ref;
    }
    let input = emelino_compiler::BackendInput::new(program).unwrap();
    // Explicit raw IR remains usable by the low-level backend contract.
    input
        .compile(&emelino_compiler::Options::default().with_heap_bytes(4096))
        .unwrap();
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("input.json"), input.to_json().unwrap()).unwrap();
    std::fs::write(scratch.0.join("config.json"), "{}").unwrap();
    let output = cli(
        &scratch.0,
        &[
            "build",
            "--input",
            "input.json",
            "--config",
            "config.json",
            "--out-dir",
            "out",
        ],
    );
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("cannot expose effect evidence"),
        "{diagnostic}"
    );
    assert!(!scratch.0.join("out").exists());
}
