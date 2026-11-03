use super::*;

fn record_type(p: &mut Program, name: &str, field: &str, ty: TypeId) -> TypeId {
    let id = p.core.enums.alloc(EnumDef {
        name: name.into(),
        params: 0,
        variants: vec![VariantDef {
            name: name.rsplit('.').next().unwrap().into(),
            fields: VariantFields::Named(vec![field.into()]),
            tys: vec![ty],
        }],
        error: false,
        external: false,
    });
    p.core.types.intern(Type::Enum(id, vec![]))
}

fn malformed_spi(case: &str) -> Program {
    let mut p = program(vec![]);
    let device = record_type(
        &mut p,
        "Embedded.Spi.SpiDevice",
        if case == "device-field-name" {
            "wrong"
        } else {
            "slot"
        },
        if case == "device-field-type" {
            TypeId::INT64
        } else {
            TypeId::INT
        },
    );
    let words = p.core.types.intern(Type::List(TypeId::INT));
    let nested = p.core.types.intern(Type::List(words));
    let operation = p.core.enums.alloc(EnumDef {
        name: "Embedded.Spi.SpiOperation".into(),
        params: 0,
        variants: ["Write", "Read", "Transfer"]
            .into_iter()
            .map(|name| {
                let empty = case == "missing-operation-field" && name == "Write";
                VariantDef {
                    name: if case == "operation-variant-order" && name != "Read" {
                        if name == "Write" { "Transfer" } else { "Write" }.into()
                    } else {
                        name.into()
                    },
                    fields: if empty {
                        VariantFields::Unit
                    } else {
                        VariantFields::Positional(1)
                    },
                    tys: if empty {
                        vec![]
                    } else {
                        vec![if name == "Read" || case == "non-list-words" {
                            TypeId::INT
                        } else {
                            words
                        }]
                    },
                }
            })
            .collect(),
        error: false,
        external: false,
    });
    let operation_type = p.core.types.intern(Type::Enum(operation, vec![]));
    let operations = p.core.types.intern(Type::List(operation_type));
    let (name, params, ret) = match case {
        "missing-arguments" => ("transaction", vec![], nested),
        "non-enum-operation" => ("transaction", vec![device, words, TypeId::INT64], nested),
        "non-list-result" => (
            "transaction",
            vec![device, operations, TypeId::INT64],
            TypeId::INT,
        ),
        "non-nested-result" => (
            "transaction",
            vec![device, operations, TypeId::INT64],
            words,
        ),
        "wrong-timeout" => ("transaction", vec![device, operations, TypeId::INT], nested),
        "create-scalar-config" => ("create_device", vec![TypeId::INT], device),
        "create-missing-config" => ("create_device", vec![], device),
        "recover-scalar-id" => ("recover", vec![TypeId::INT], TypeId::UNIT),
        "recover-missing-id" => ("recover", vec![], TypeId::UNIT),
        _ => (
            "transaction",
            vec![device, operations, TypeId::INT64],
            nested,
        ),
    };
    p.core.effects.alloc(EffectDef {
        name: "Embedded.Spi.Spi".into(),
        decl: "Embedded.Spi.Spi".into(),
        args: vec![],
        label_of: None,
        ops: vec![OpDef {
            name: name.into(),
            suspend: false,
            type_params: 0,
            params: params.clone(),
            ret,
            fails: true,
        }],
    });
    let abi_return = p.core.types.intern(Type::Tuple(vec![
        TypeId::INT,
        TypeId::INT,
        TypeId::INT,
        ret,
    ]));
    let mut external = Function::new(&format!("Embedded.Board.spi_{name}"));
    external.ret = abi_return;
    for &ty in &params {
        let parameter = typed_variable(&mut external, ty);
        external.params.push(parameter);
    }
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global(format!("__emela_peripheral_spi_{name}")),
        }],
    });
    let target = p.functions.alloc(external);
    let mut entry = simple();
    entry.ret = abi_return;
    let arguments = params
        .iter()
        .map(|&ty| {
            let parameter = typed_variable(&mut entry, ty);
            entry.params.push(parameter);
            parameter.into()
        })
        .collect();
    let result = typed_variable(&mut entry, abi_return);
    let call = binding(
        &mut entry,
        Some(result),
        Value::Call {
            callee: Callee::Direct(target),
            args: arguments,
            suspends: false,
            on_fail: None,
        },
    );
    entry.seqs[entry.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(entry);
    p
}

fn typed_variable(function: &mut Function, ty: TypeId) -> VarId {
    let variable = super::variable(function, ty);
    if function.vars[variable].repr == Repr::Void && ty != TypeId::UNIT {
        function.vars[variable].repr = Repr::Ref;
    }
    variable
}

#[test]
fn compound_spi_saved_ir_rejects_consistently_changed_but_unsupported_shapes() {
    for case in [
        "missing-arguments",
        "non-enum-operation",
        "missing-operation-field",
        "non-list-words",
        "operation-variant-order",
        "non-list-result",
        "non-nested-result",
        "wrong-timeout",
        "device-field-name",
        "device-field-type",
        "create-scalar-config",
        "create-missing-config",
        "recover-scalar-id",
        "recover-missing-id",
    ] {
        // These are valid owned IR definitions: the operation and private external
        // agree. Replay must report an unsupported ABI instead of panicking or
        // producing C that accesses nonexistent payload members.
        let input = crate::BackendInput::new(malformed_spi(case)).unwrap();
        let input = crate::BackendInput::from_json(&input.to_json().unwrap()).unwrap();
        let errors = input
            .compile(&Options::default().with_heap_bytes(4096))
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == ErrorKind::Unsupported
                    && error
                        .message
                        .contains("invalid compound SPI board binding shape")),
            "{case}: {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .all(|error| error.kind != ErrorKind::InvalidFlowIr),
            "{case}: {errors:#?}"
        );
    }
}
