use super::*;

#[test]
fn discarded_deferreds_reuse_one_slot_in_a_long_lived_scope() {
    for discard in [false, true] {
        let mut p = program(vec![]);
        let mut f = simple();
        f.suspends = true;
        let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
        let left = variable(&mut f, TypeId::INT);
        let less = variable(&mut f, TypeId::INT);
        let zero = variable(&mut f, TypeId::BOOL);
        let dst = if discard {
            None
        } else {
            Some(aggregate_variable(&mut f, TypeId::ANY))
        };
        let make = binding(
            &mut f,
            dst,
            Value::Fiber {
                op: FiberOp::Deferred,
                args: vec![scope.into()],
                on_fail: None,
            },
        );
        let check = binding(
            &mut f,
            Some(zero),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::I32,
                args: vec![left.into(), Const::Int(0).into()],
            },
        );
        let end = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(Const::Int(42).into())),
        });
        let more = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Yield(None),
        });
        let branch = f.stmts.alloc(Stmt::If {
            dst: None,
            cond: zero.into(),
            then: end,
            else_: more,
        });
        let subtract = binding(
            &mut f,
            Some(less),
            Value::Prim {
                op: PrimOp::Sub,
                ty: PrimTy::I32,
                args: vec![left.into(), Const::Int(1).into()],
            },
        );
        let body = f.seqs.alloc(Seq {
            stmts: vec![make, check, branch, subtract],
            term: Term::Recur(vec![less.into()]),
        });
        let loop_ = f.stmts.alloc(Stmt::Loop {
            dst: None,
            vars: vec![left],
            inits: vec![Const::Int(999).into()],
            body,
        });
        let body = f.seqs.alloc(Seq {
            stmts: vec![loop_],
            term: Term::Return(Some(Const::Int(42).into())),
        });
        scope_function(&mut p, &mut f, body, scope);
        p.functions.alloc(f);
        let output = crate::backend::compile_saved_program(
            p,
            &Options::default()
                .with_heap_bytes(4096)
                .with_frame_bytes(1024)
                .with_deferred_capacity(1),
        )
        .unwrap();
        execute_answer(output);
    }
}
