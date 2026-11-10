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
            json!("6021115103dd830e0efd9bb96a56a0ca97859329"),
        ),
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
    for array in [false, true] {
        let mut p = program();
        let next = TypeId::from_index(p.core.types.len());
        p.core.types.types.push(if array {
            Type::Array(next)
        } else {
            Type::List(next)
        });
        assert!(
            BackendInput::new(p).unwrap_err()[0]
                .message
                .contains("cyclic type")
        );
    }
    let mut json: Json = serde_json::from_str(&saved).unwrap();
    json.pointer_mut("/program/core/types/types")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(json!({"Array": u32::MAX}));
    assert!(
        BackendInput::from_json(&json.to_string()).unwrap_err()[0]
            .message
            .contains("type ID is out of bounds")
    );
}

/// Keep the callback behind an enum field: catalog substitution reached a panic
/// before runtime value validation could reject a malformed function set.
fn callback_field_program() -> (Program, TypeId) {
    let mut p = program();
    let empty = p.core.types.intern(Type::Set(SetSummary::default()));
    let callback = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: Some(FnSets {
            errors: empty,
            effects: empty,
        }),
    }));
    let wrapper = p.core.enums.alloc(EnumDef {
        name: "Callback".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Callback".into(),
            fields: VariantFields::Positional(1),
            tys: vec![callback],
        }],
        error: false,
        external: false,
    });
    let wrapper = p.core.types.intern(Type::Enum(wrapper, vec![]));
    p.functions
        .iter_mut()
        .next()
        .unwrap()
        .1
        .vars
        .alloc(VarData {
            ty: wrapper,
            repr: Repr::Ref,
            hint: "unused_callback".into(),
        });
    (p, callback)
}

#[test]
fn invalid_function_sets_are_rejected_before_enum_field_substitution() {
    let (mut p, callback) = callback_field_program();
    let parameter = p.core.types.intern(Type::Param(0));
    let input = BackendInput::new(p.clone()).unwrap();
    let saved = input.to_json().unwrap();
    let options = Options::default().with_entry("");
    assert_eq!(
        input.compile(&options).unwrap(),
        BackendInput::from_json(&saved)
            .unwrap()
            .compile(&options)
            .unwrap()
    );
    for (field, kind) in [("errors", "error"), ("effects", "effect")] {
        for invalid_type in [TypeId::INT, parameter] {
            let diagnostic = format!("function {kind} set must reference a set type");
            let mut changed = p.clone();
            let Type::Fn(function) = &mut changed.core.types.types[callback.index()] else {
                unreachable!()
            };
            let sets = function.sets.as_mut().unwrap();
            if field == "errors" {
                sets.errors = invalid_type;
            } else {
                sets.effects = invalid_type;
            }
            let errors = BackendInput::new(changed).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains(&diagnostic))
            );

            let mut changed: Json = serde_json::from_str(&saved).unwrap();
            changed["program"]["core"]["types"]["types"][callback.index()]["Fn"]["sets"][field] =
                json!(invalid_type);
            let errors = BackendInput::from_json(&changed.to_string()).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains(&diagnostic))
            );
        }
    }
}

#[test]
fn generic_function_set_declarations_survive_input_roundtrip() {
    let mut p = program();
    let parameter = p.core.types.intern(Type::Param(0));
    let generic_set = p.core.types.intern(Type::Set(SetSummary {
        params: vec![parameter],
        ..SetSummary::default()
    }));
    let callback = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: true,
        suspends: true,
        ev: true,
        sets: Some(FnSets {
            errors: generic_set,
            effects: generic_set,
        }),
    }));
    p.core.enums.alloc(EnumDef {
        name: "GenericCallback".into(),
        params: 1,
        variants: vec![VariantDef {
            name: "GenericCallback".into(),
            fields: VariantFields::Positional(1),
            tys: vec![callback],
        }],
        error: false,
        external: false,
    });
    let input = BackendInput::new(p).unwrap();
    let saved = input.to_json().unwrap();
    let replay = BackendInput::from_json(&saved).unwrap();
    assert_eq!(saved, replay.to_json().unwrap());
    let options = Options::default().with_entry("");
    assert_eq!(
        input.compile(&options).unwrap(),
        replay.compile(&options).unwrap()
    );

    // A generic set may specialize to either an explicit set or upstream's
    // Unit placeholder. Neither changes the invariant that E/R are Set nodes.
    for argument in [generic_set, TypeId::UNIT] {
        let mut types = replay.program().core.types.clone();
        let specialized = types.subst(callback, &[argument]);
        let Type::Fn(function) = &types[specialized] else {
            panic!("function type lost during substitution")
        };
        if argument == TypeId::UNIT {
            assert!(function.sets.is_none());
            assert!(function.fails && function.suspends && function.ev);
        } else {
            assert!(function.sets.is_some());
        }
    }
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
