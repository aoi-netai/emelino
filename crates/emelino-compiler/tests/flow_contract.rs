use emelino_compiler::{
    Constant, FORMAT, FlowArtifact, FlowFunction, Producer, SCHEMA_VERSION, Terminator,
    UPSTREAM_REVISION, compile, from_json, to_json,
};

fn artifact() -> FlowArtifact {
    FlowArtifact {
        format: FORMAT.into(),
        schema_version: SCHEMA_VERSION,
        producer: Producer {
            name: "emela".into(),
            revision: UPSTREAM_REVISION.into(),
        },
        stage: "flow".into(),
        functions: vec![FlowFunction {
            name: "source_answer".into(),
            exports: vec!["answer".into()],
            body: Terminator::Return {
                value: Constant::Int { value: 42 },
            },
        }],
    }
}

#[test]
fn roundtrip_preserves_export_aliases_and_values() {
    let mut input = artifact();
    input.functions[0].exports.push("answer_alias".into());
    assert_eq!(from_json(&to_json(&input).unwrap()).unwrap(), input);
    let c = compile(&input).unwrap();
    assert!(c.header.contains("int32_t answer(void);"));
    assert!(c.header.contains("int32_t answer_alias(void);"));
    assert!(!c.source.contains("source_answer"));
}

#[test]
fn rejects_incompatible_envelopes() {
    let original = serde_json::to_value(artifact()).unwrap();
    for (path, value) in [
        ("/format", serde_json::json!("emela.flow")),
        ("/schema_version", serde_json::json!(2)),
        ("/stage", serde_json::json!("ref_counted")),
        ("/producer/name", serde_json::json!("another-frontend")),
        ("/producer/revision", serde_json::json!("different")),
        (
            "/producer/revision",
            serde_json::json!("6021115103dd830e0efd9bb96a56a0ca97859329"),
        ),
        (
            "/producer/revision",
            serde_json::json!("ec1031cb0b98386baf0e9b0ced67e8829bb2c529"),
        ),
    ] {
        let mut json = original.clone();
        *json.pointer_mut(path).unwrap() = value;
        assert!(from_json(&json.to_string()).is_err(), "accepted {path}");
    }
    let mut input = artifact();
    input.stage = "heap".into();
    assert!(compile(&input).is_err());
    assert!(to_json(&input).is_err());
}

#[test]
fn rejects_unrepresented_fields_and_operations() {
    let original = serde_json::to_value(artifact()).unwrap();
    for path in [
        "",
        "/producer",
        "/functions/0",
        "/functions/0/body",
        "/functions/0/body/value",
    ] {
        let mut json = original.clone();
        json.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unsupported".into(), serde_json::json!(true));
        assert!(from_json(&json.to_string()).is_err(), "accepted {path}");
    }
    for (path, value) in [
        ("/functions/0/body/kind", serde_json::json!("fail")),
        ("/functions/0/body/value/kind", serde_json::json!("float")),
        (
            "/functions/0/body/value/value",
            serde_json::json!(2147483648_i64),
        ),
        (
            "/functions/0/body/value/value",
            serde_json::json!(-2147483649_i64),
        ),
        ("/functions/0/body/value/value", serde_json::json!(1.5)),
        ("/functions/0/body/value/value", serde_json::json!("42")),
    ] {
        let mut json = original.clone();
        *json.pointer_mut(path).unwrap() = value;
        assert!(from_json(&json.to_string()).is_err(), "accepted {path}");
    }
}

#[test]
fn rejects_missing_and_duplicate_definitions() {
    let mut input = artifact();
    input.functions.clear();
    assert!(compile(&input).is_err());
    let mut input = artifact();
    input.functions[0].exports.clear();
    assert!(compile(&input).is_err());
    let mut input = artifact();
    input.functions[0].name.clear();
    assert!(compile(&input).is_err());
    let mut input = artifact();
    input.functions.push(input.functions[0].clone());
    assert!(compile(&input).unwrap_err().contains("duplicate Flow"));
    input.functions[1].name = "other".into();
    assert!(compile(&input).unwrap_err().contains("duplicate C export"));
    let mut input = artifact();
    input.functions[0].exports.push("answer".into());
    assert!(compile(&input).unwrap_err().contains("duplicate C export"));
}

#[test]
fn rejects_invalid_or_reserved_c_symbols() {
    for symbol in [
        "",
        "42answer",
        "a-b",
        "a\n",
        "答え",
        "_private",
        "__reserved",
        "main",
        "return",
        "void",
        "int32_t",
        "uint_fast64_t",
        "INT32_C",
        "UINT64_MAX",
        "SIZE_MAX",
        "EMELINO_PROGRAM_H",
        "answer(void) { return 0; }",
    ] {
        let mut input = artifact();
        input.functions[0].exports = vec![symbol.into()];
        assert!(compile(&input).is_err(), "accepted {symbol:?}");
    }
}

#[test]
fn emitted_c_preserves_integer_extremes_at_o0_and_o2() {
    let mut input = artifact();
    input.functions = [
        ("minimum", i32::MIN),
        ("maximum", i32::MAX),
        ("negative", -1),
        ("zero", 0),
        ("different", 17),
    ]
    .into_iter()
    .map(|(name, value)| FlowFunction {
        name: name.into(),
        exports: vec![name.into()],
        body: Terminator::Return {
            value: Constant::Int { value },
        },
    })
    .collect();
    let c = compile(&input).unwrap();
    let dir = TempDir::new();
    std::fs::write(dir.0.join("program.c"), c.source).unwrap();
    std::fs::write(dir.0.join("program.h"), c.header).unwrap();
    std::fs::write(
        dir.0.join("host.c"),
        "#include \"program.h\"\nint main(void) {\n\
         return minimum() == INT32_MIN && maximum() == INT32_MAX &&\n\
         negative() == -1 && zero() == 0 && different() == 17 ? 0 : 1;\n}\n",
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = dir.0.join(format!("host{}", &optimization[1..]));
        let output = std::process::Command::new("cc")
            .current_dir(&dir.0)
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
                optimization,
            ])
            .args(["program.c", "host.c", "-o"])
            .arg(&executable)
            .output()
            .expect("the C integration test requires cc");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            std::process::Command::new(executable)
                .status()
                .unwrap()
                .success()
        );
    }
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "emelino-compiler-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
