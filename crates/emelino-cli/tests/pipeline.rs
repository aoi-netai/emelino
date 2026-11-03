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
                .arg(format!("{output}/program.c"))
                .arg("host.c")
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("a C compiler is required for the pipeline test"),
        );
        success(Command::new(executable).output().unwrap());
    }
}

#[test]
fn upstream_flow_survives_source_removal_and_links_from_c() {
    let scratch = Scratch::new();
    std::fs::write(
        scratch.0.join("answer.emel"),
        include_str!("../../../examples/answer.emel"),
    )
    .unwrap();
    success(cli(
        &scratch.0,
        &["flow", "answer.emel", "--out-dir", "flow"],
    ));
    let dump = std::fs::read_to_string(scratch.0.join("flow/program.flow.txt")).unwrap();
    assert!(dump.contains("42"), "{dump}");
    assert!(!scratch.0.join("flow/program.c").exists());
    std::fs::remove_file(scratch.0.join("answer.emel")).unwrap();
    // This invocation has only a saved artifact, with no Emela source to compile.
    success(cli(
        &scratch.0,
        &[
            "build",
            "--flow",
            "flow/program.flow.json",
            "--out-dir",
            "c",
        ],
    ));
    run_c(&scratch.0, "c", include_str!("../../../examples/host.c"));
}

#[test]
fn source_build_preserves_export_alias_and_integer_value() {
    let scratch = Scratch::new();
    std::fs::write(
        scratch.0.join("different.emel"),
        "@export(c, \"read_value\")\npub fn answer() -> Int { 17 }\n",
    )
    .unwrap();
    success(cli(
        &scratch.0,
        &["build", "different.emel", "--out-dir", "out"],
    ));
    assert!(scratch.0.join("out/program.flow.json").is_file());
    run_c(
        &scratch.0,
        "out",
        "#include \"program.h\"\nint main(void) { return read_value() == 17 ? 0 : 1; }\n",
    );
}

#[test]
fn frontend_errors_and_unsupported_flow_do_not_emit_c() {
    let scratch = Scratch::new();
    let inputs = [
        "@export(c, \"answer\")\npub fn answer() -> Int {",
        "@export(c, \"answer\")\npub fn answer() -> Int { \"not an Int\" }",
        "@export(c, \"answer\")\npub fn answer(value: Int) -> Int { value }",
        "pub fn answer() -> Int { 42 }",
        "@export(c, \"answer\")\npub fn answer() -> Int { helper() }\nfn helper() -> Int { 42 }",
    ];
    for (index, source) in inputs.iter().enumerate() {
        std::fs::write(scratch.0.join("input.emel"), source).unwrap();
        let directory = format!("case{index}");
        let result = cli(
            &scratch.0,
            &["build", "input.emel", "--out-dir", &directory],
        );
        assert!(!result.status.success(), "unexpectedly accepted {source}");
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.starts_with("emelino: "), "{error}");
        assert!(!error.contains("panicked"), "{error}");
        assert!(!scratch.0.join(&directory).exists());
    }
}

#[test]
fn invalid_saved_flow_and_bad_arguments_fail_without_outputs() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("bad.json"), "{not json}").unwrap();
    for args in [
        vec!["build", "--flow", "bad.json", "--out-dir", "out"],
        vec!["build", "--out-dir", "out"],
        vec!["flow", "--flow", "bad.json", "--out-dir", "out"],
        vec!["build", "a.emel", "--flow", "bad.json", "--out-dir", "out"],
        vec!["build", "a.emel", "--out-dir", "--flow"],
    ] {
        let result = cli(&scratch.0, &args);
        assert!(!result.status.success(), "{args:?}");
        assert!(!scratch.0.join("out").exists());
    }
}
