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

#[test]
fn infallible_scope_accepts_conservative_body_flags_but_keeps_escaping_failures_invalid() {
    let mut p = program(vec![]);
    let ty = p.core.types.intern(Type::Fn(FnType {
        params: vec![TypeId::FIBER_SCOPE],
        ret: TypeId::INT,
        fails: true,
        suspends: true,
        ev: false,
        sets: None,
    }));
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let erased = aggregate_variable(&mut f, TypeId::ANY);
    let value = variable(&mut f, TypeId::INT);
    let literal = binding(&mut f, Some(value), Value::Const(Const::Int(42)));
    let box_value = binding(&mut f, Some(erased), Value::Coerce(value));
    let body = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    });
    let closure = aggregate_variable(&mut f, ty);
    let make = binding(
        &mut f,
        Some(closure),
        Value::Closure(Box::new(Closure {
            params: vec![scope],
            captures: vec![erased],
            body,
            suspends: true,
        })),
    );
    let result = variable(&mut f, TypeId::INT);
    let enter = binding(
        &mut f,
        Some(result),
        Value::Fiber {
            op: FiberOp::Scope { fails: false },
            args: vec![closure.into()],
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![literal, box_value, make, enter],
        term: Term::Return(Some(result.into())),
    };
    let id = p.functions.alloc(f);
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_frame_bytes(1024);
    let output = crate::backend::compile_saved_program(p.clone(), &options).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);return result==42?0:2;}\n",
        0,
    );
    p.functions[id].seqs[body].term = Term::Fail(erased.into());
    let errors = crate::compile_program(p, &options).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("Scope body or failure ABI is invalid")),
        "{errors:?}"
    );
}
