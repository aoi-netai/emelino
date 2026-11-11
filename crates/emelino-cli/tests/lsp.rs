//! Actual Content-Length framed stdin/stdout integration, with all SDK paths absent.
#[test]
fn language_server_stdio_workflows() {
    let result = std::process::Command::new("python3")
        .arg("-B")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/lsp/test_stdio.py"
        ))
        .env("EMELINO_TEST_BIN", env!("CARGO_BIN_EXE_emelino"))
        .output()
        .expect("Python 3 is required for the repository's protocol integration tests");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
