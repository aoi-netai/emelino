//! Materialize function-value channel conversions implicit in upstream Flow.
use super::*;

pub(super) fn normalize(program: &mut Program) {
    let signatures: HashMap<_, Vec<_>> = program
        .functions
        .iter()
        .map(|(id, f)| (id, f.params.iter().map(|p| f.vars[*p].ty).collect()))
        .collect();
    for (_, function) in program.functions.iter_mut() {
        let mut before: HashMap<StmtId, Vec<StmtId>> = HashMap::new();
        for (statement, value) in function.stmts.clone().iter() {
            let Stmt::Let { value, .. } = value else {
                continue;
            };
            let (arguments, expected) = match value {
                Value::Call { callee, args, .. } => {
                    let expected = match callee {
                        Callee::Direct(id) => signatures[id].clone(),
                        Callee::Indirect(variable) => {
                            let Type::Fn(ft) = program.core.ty(function.vars[*variable].ty) else {
                                continue;
                            };
                            let mut params: Vec<_> = ft
                                .params
                                .iter()
                                .copied()
                                .filter(|ty| !matches!(*ty, TypeId::UNIT | TypeId::NEVER))
                                .collect();
                            params.extend(ft.ev.then_some(TypeId::EVIDENCE));
                            params
                        }
                        Callee::Op { effect, op, .. } => {
                            program.core.effects[*effect].ops[*op].params.clone()
                        }
                    };
                    (args, expected)
                }
                Value::Fiber {
                    op: FiberOp::Scope { fails: false },
                    args,
                    ..
                } => {
                    let [Operand::Var(body)] = args.as_slice() else {
                        continue;
                    };
                    let Type::Fn(mut ft) = program.core.ty(function.vars[*body].ty).clone() else {
                        continue;
                    };
                    if !ft.fails || !infallible_closure(function, *body) {
                        continue;
                    }
                    // Generic Scope synthesis can retain a failure flag on its
                    // body even when no failure can escape this closure. Keep
                    // the body's original ABI and use the checked existing
                    // coercion adapter at the infallible Scope boundary.
                    ft.fails = false;
                    (args, vec![program.core.types.intern(Type::Fn(ft))])
                }
                _ => continue,
            };
            for (index, (argument, expected)) in arguments.iter().zip(expected).enumerate() {
                let Operand::Var(source) = argument else {
                    continue;
                };
                let actual = function.vars[*source].ty;
                if actual == expected {
                    continue;
                }
                let (Type::Fn(from), Type::Fn(to)) =
                    (program.core.ty(actual), program.core.ty(expected))
                else {
                    continue;
                };
                let mut widened = from.clone();
                widened.fails = to.fails;
                if &widened != to {
                    continue;
                }
                let scope = matches!(
                    value,
                    Value::Fiber {
                        op: FiberOp::Scope { fails: false },
                        ..
                    }
                );
                if from.fails && !to.fails && !scope {
                    continue;
                }
                let adapted = function.vars.alloc(VarData {
                    hint: "function_channel_argument".into(),
                    ty: expected,
                    repr: Repr::Ref,
                });
                let conversion = function.stmts.alloc(Stmt::Let {
                    dst: Some(adapted),
                    value: Value::Coerce(*source),
                    on_defect: None,
                    on_suspend: None,
                });
                before.entry(statement).or_default().push(conversion);
                let Stmt::Let {
                    value: Value::Call { args, .. } | Value::Fiber { args, .. },
                    ..
                } = &mut function.stmts[statement]
                else {
                    unreachable!()
                };
                args[index] = adapted.into();
            }
        }
        for (_, sequence) in function.seqs.iter_mut() {
            sequence.stmts = sequence
                .stmts
                .iter()
                .flat_map(|id| {
                    before
                        .get(id)
                        .into_iter()
                        .flatten()
                        .copied()
                        .chain(std::iter::once(*id))
                })
                .collect();
        }
    }
}

fn infallible_closure(function: &Function, variable: VarId) -> bool {
    function
        .stmts
        .iter()
        .find_map(|(_, statement)| match statement {
            Stmt::Let {
                dst: Some(dst),
                value: Value::Closure(closure),
                ..
            } if *dst == variable => Some(!escaping_failure(
                function,
                closure.body,
                &mut HashSet::new(),
            )),
            _ => None,
        })
        .unwrap_or(false)
}

// Closure construction does not execute the nested body. Local failure catches
// are inspected separately; only propagation escapes the current closure.
fn escaping_failure(function: &Function, body: SeqId, seen: &mut HashSet<SeqId>) -> bool {
    if !seen.insert(body) {
        return false;
    }
    let sequence = &function.seqs[body];
    if matches!(sequence.term, Term::Fail(_) | Term::Unwind) {
        return true;
    }
    sequence.stmts.iter().any(|id| {
        let mut children = Vec::new();
        match &function.stmts[*id] {
            Stmt::Let {
                value, on_defect, ..
            } => {
                children.extend(on_defect);
                if let Value::Call { on_fail, .. } | Value::Fiber { on_fail, .. } = value {
                    match on_fail {
                        Some(OnFail::Propagate) => return true,
                        Some(OnFail::Handle { body, .. }) => children.push(body),
                        _ => {}
                    }
                }
            }
            Stmt::If { then, else_, .. } => children.extend([then, else_]),
            Stmt::Switch { cases, default, .. } => {
                children.extend(cases.iter().map(|case| &case.body));
                children.extend(default);
            }
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => children.push(body),
            Stmt::Protect { body, cleanup, .. } => children.extend([body, cleanup]),
            _ => {}
        }
        children
            .into_iter()
            .any(|child| escaping_failure(function, *child, seen))
    })
}
