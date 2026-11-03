//! Representation boundaries emitted by the upstream frontend.
use super::*;

#[test]
fn scalar_enum_equality_preserves_nominal_types_and_rejects_arithmetic() {
    let mut p = program(vec![]);
    let definition = EnumDef {
        name: "Ordering".into(),
        params: 0,
        error: false,
        external: false,
        variants: vec![
            VariantDef {
                name: "Less".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
            VariantDef {
                name: "Greater".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
    };
    let enum_id = p.core.enums.alloc(definition.clone());
    let ty = p.core.types.enum_type(enum_id, vec![]);
    let mut f = Function::new("enum_equal");
    f.exported = true;
    f.export_symbol = Some("enum_equal".into());
    f.ret = TypeId::BOOL;
    let left = f.vars.alloc(VarData {
        hint: "left".into(),
        ty,
        repr: Repr::I32,
    });
    let right = f.vars.alloc(VarData {
        hint: "right".into(),
        ty,
        repr: Repr::I32,
    });
    f.params = vec![left, right];
    let result = variable(&mut f, TypeId::BOOL);
    let comparison = binding(
        &mut f,
        Some(result),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![left.into(), right.into()],
        },
    );
    f.body = Some(f.seqs.alloc(Seq {
        stmts: vec![comparison],
        term: Term::Return(Some(result.into())),
    }));
    let function = p.functions.alloc(f);
    let input = crate::BackendInput::new(p.clone()).unwrap();
    let output = crate::BackendInput::from_json(&input.to_json().unwrap())
        .unwrap()
        .compile(&Options::default())
        .unwrap();
    execute(
        &output,
        "#include \"emela_embedded.h\"\nint main(void){return enum_equal(0,0)==1 && enum_equal(0,1)==0 ? 0 : 1;}\n",
        0,
    );
    let mut bad = p.clone();
    if let Stmt::Let {
        value: Value::Prim { op, .. },
        ..
    } = &mut bad.functions[function].stmts[comparison]
    {
        *op = PrimOp::Add;
    }
    assert!(crate::compile_program(bad, &Options::default()).is_err());
    let other = p.core.enums.alloc(EnumDef {
        name: "Other".into(),
        ..definition
    });
    let other_ty = p.core.types.enum_type(other, vec![]);
    p.functions[function].vars[right].ty = other_ty;
    assert!(
        crate::compile_program(p, &Options::default())
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("different types"))
    );
}
