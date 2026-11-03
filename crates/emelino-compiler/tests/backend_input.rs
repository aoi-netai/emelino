use emelino_compiler::{BackendInput, Options, ir::*};
use la_arena::{Arena, Idx};
use serde_json::{Value as Json, json};

fn program() -> Program {
    let mut f = Function::new("answer");
    f.exported = true;
    f.export_symbol = Some("answer".into());
    f.ret = TypeId::INT;
    f.body = Some(f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    }));
    Program {
        core: Definitions::default(),
        functions: [f].into_iter().collect(),
        error_shows: vec![],
        const_function_groups: vec![],
        has_scopes: false,
    }
}

#[test]
fn saved_input_generates_the_same_files_without_a_frontend() {
    let options = Options::default().with_entry("");
    let input = BackendInput::new(program()).unwrap();
    let saved = input.to_json().unwrap();
    let replay = BackendInput::from_json(&saved).unwrap();
    assert_eq!(saved, replay.to_json().unwrap());
    let output = replay.compile(&options).unwrap();
    assert_eq!(input.compile(&options).unwrap(), output);
    assert!(output.header.contains("int32_t answer(void);"));
    assert!(output.files.iter().any(|f| f.path.ends_with(".json")));
}

#[test]
fn serialization_preserves_full_integer_and_float_bit_patterns() {
    let mut p = program();
    let values = [
        Const::Float(f64::from_bits(0x7ff8_0000_0000_0042)),
        Const::Float(f64::NEG_INFINITY),
        Const::Float(-0.0),
        Const::Int64(i64::MIN),
        Const::Int64(i64::MAX),
        Const::String("nul\0日本語".into()),
    ];
    for (i, value) in values.iter().enumerate() {
        let mut f = Function::new(&format!("value_{i}"));
        f.ret = match value {
            Const::Float(_) => TypeId::FLOAT,
            Const::Int64(_) => TypeId::INT64,
            _ => TypeId::STRING,
        };
        f.body = Some(f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(value.clone().into())),
        }));
        p.functions.alloc(f);
    }
    let saved = BackendInput::new(p).unwrap().to_json().unwrap();
    let replay = BackendInput::from_json(&saved).unwrap();
    for ((_, f), expected) in replay.program().functions.iter().skip(1).zip(values) {
        let Term::Return(Some(Operand::Const(actual))) = &f.seqs[f.body.unwrap()].term else {
            panic!("lost value")
        };
        match (actual, expected) {
            (Const::Float(actual), Const::Float(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits())
            }
            (actual, expected) => assert_eq!(*actual, expected),
        }
    }
}

#[test]
fn incompatible_envelopes_unknown_fields_and_operations_are_rejected() {
    let original: Json =
        serde_json::from_str(&BackendInput::new(program()).unwrap().to_json().unwrap()).unwrap();
    for (path, value) in [
        ("/format", json!("emelino.flow.minimal")),
        ("/schema_version", json!(2)),
        ("/producer_revision", json!("latest")),
        (
            "/producer_revision",
            json!("ec1031cb0b98386baf0e9b0ced67e8829bb2c529"),
        ),
        ("/stage", json!("ref_counted")),
        ("/required_features", json!(["future_operation"])),
        ("/program/functions/0/seqs/0/term", json!({"Future":null})),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(
            BackendInput::from_json(&changed.to_string()).is_err(),
            "accepted {path}"
        );
    }
    for path in [
        "",
        "/program",
        "/program/core",
        "/program/functions/0",
        "/program/functions/0/seqs/0",
    ] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(42));
        assert!(
            BackendInput::from_json(&changed.to_string()).is_err(),
            "accepted field at {path}"
        );
    }
}

#[test]
fn saved_metadata_cannot_override_lowering_results() {
    let mut p = program();
    p.has_scopes = true;
    let id = p.functions.iter().next().unwrap().0;
    let f = &mut p.functions[id];
    f.sequence_positions.insert(f.body.unwrap(), 1000);
    let input = BackendInput::new(p).unwrap();
    assert!(!input.program().has_scopes);
    assert!(input.program().functions[id].sequence_positions.is_empty());
    let saved = input.to_json().unwrap();
    assert!(!saved.contains("has_scopes"));
    assert!(!saved.contains("has_param"));
    assert!(!saved.contains("sequence_positions"));
    for (path, field, value) in [
        ("/program", "has_scopes", json!(true)),
        ("/program/core/types", "has_param", json!([false])),
        (
            "/program/functions/0",
            "sequence_positions",
            json!({"0":1000}),
        ),
    ] {
        let mut json: Json = serde_json::from_str(&saved).unwrap();
        json.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(field.into(), value);
        assert!(
            BackendInput::from_json(&json.to_string()).is_err(),
            "accepted cached {field}"
        );
    }
}

#[test]
fn invalid_references_and_cycles_are_rejected_before_lowering() {
    let saved = BackendInput::new(program()).unwrap().to_json().unwrap();
    for (path, value) in [
        ("/program/functions/0/ret", json!(u32::MAX)),
        ("/program/functions/0/body", json!(u32::MAX)),
        ("/program/functions/0/params", json!([u32::MAX])),
        (
            "/program/functions/0/seqs/0/term",
            json!({"Return":{"Var":u32::MAX}}),
        ),
        ("/program/functions/0/seqs/0/stmts", json!([u32::MAX])),
        ("/program/core/types/types/0", json!("Bool")),
    ] {
        let mut json: Json = serde_json::from_str(&saved).unwrap();
        *json.pointer_mut(path).unwrap() = value;
        assert!(
            BackendInput::from_json(&json.to_string()).is_err(),
            "accepted {path}"
        );
    }
    let mut p = program();
    let id = p.functions.iter().next().unwrap().0;
    let f = &mut p.functions[id];
    let body = f.body.unwrap();
    let stmt = f.stmts.alloc(Stmt::If {
        dst: None,
        cond: Const::Bool(true).into(),
        then: body,
        else_: body,
    });
    f.seqs[body].stmts.push(stmt);
    assert!(
        BackendInput::new(p).unwrap_err()[0]
            .message
            .contains("cyclic sequence")
    );
    let mut p = program();
    let next = TypeId::from_index(p.core.types.len());
    p.core.types.types.push(Type::List(next));
    assert!(
        BackendInput::new(p).unwrap_err()[0]
            .message
            .contains("cyclic type")
    );
}

#[test]
fn semantic_type_mismatches_remain_compile_errors_after_roundtrip() {
    let mut p = program();
    let id = p.functions.iter().next().unwrap().0;
    let f = &mut p.functions[id];
    f.seqs[f.body.unwrap()].term = Term::Return(Some(Const::Bool(true).into()));
    let input = BackendInput::new(p).unwrap();
    let restored = BackendInput::from_json(&input.to_json().unwrap()).unwrap();
    let errors = restored.compile(&Options::default()).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("type mismatch")));
}

#[test]
fn direct_compile_api_validates_arena_ids() {
    let mut p = program();
    let id = p.functions.iter().next().unwrap().0;
    p.functions[id].body = Some(Idx::from_raw(u32::MAX.into()));
    assert!(emelino_compiler::compile_program(p, &Options::default()).is_err());
    let mut p = program();
    p.functions[id].vars = Arena::new();
    p.functions[id].params.push(Idx::from_raw(u32::MAX.into()));
    assert!(emelino_compiler::compile_program(p, &Options::default()).is_err());
}
