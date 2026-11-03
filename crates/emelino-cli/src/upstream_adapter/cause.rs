//! Lower infallible Cause.raise to the existing backend landing contract.
//! Adapted from emela-codegen-emb/src/input/upstream/cause.rs at
//! c34c0aeefdd966d854371b3110db00a47798751d (Apache-2.0).
use emelino_compiler::ir::{self as d, FiberOp, Stmt, Term, Type, TypeId, TypeTable, Value};

// Upstream permits CauseRaise without a failure edge when the error set is
// empty. The existing runtime still checks the cause's kind; a local landing
// preserves this behavior without changing upstream Flow or Core.
pub(super) fn normalize(module: &mut d::Program) -> Result<(), String> {
    fn visit(
        types: &TypeTable,
        f: &mut d::Function,
        seq: d::SeqId,
        fails: bool,
        landings: &mut Vec<(d::SeqId, d::VarId)>,
    ) {
        let statements = f.seqs[seq].stmts.clone();
        for (position, id) in statements.into_iter().enumerate() {
            let stmt = f.stmts[id].clone();
            match stmt {
                Stmt::Let {
                    dst,
                    value: Value::Closure(closure),
                    ..
                } => {
                    let Some(dst) = dst else {
                        continue;
                    };
                    let Type::Fn(ft) = &types[f.vars[dst].ty] else {
                        continue;
                    };
                    visit(types, f, closure.body, ft.fails, landings);
                }
                Stmt::Let {
                    dst: None,
                    value:
                        Value::Fiber {
                            op: FiberOp::CauseRaise,
                            on_fail: None | Some(d::OnFail::Propagate),
                            ..
                        },
                    ..
                } if !fails => {
                    let error = f.vars.alloc(d::VarData {
                        hint: "empty_cause_failure".into(),
                        ty: TypeId::ANY,
                        repr: d::Repr::Ref,
                    });
                    let label = f.labels.alloc(d::LabelData {
                        hint: "empty_cause_failure".into(),
                    });
                    if let Stmt::Let {
                        value: Value::Fiber { op, on_fail, .. },
                        ..
                    } = &mut f.stmts[id]
                    {
                        *op = FiberOp::CauseRaise;
                        *on_fail = Some(d::OnFail::Break(label));
                    }
                    let body = f.seqs.alloc(d::Seq {
                        stmts: vec![id],
                        term: Term::Unreachable,
                    });
                    let block = f.stmts.alloc(Stmt::Block {
                        dst: Some(error),
                        label,
                        body,
                    });
                    let body = f.seqs.alloc(d::Seq {
                        stmts: vec![block],
                        term: Term::Unreachable,
                    });
                    landings.push((body, error));
                    let label = f.labels.alloc(d::LabelData {
                        hint: "raise_empty_cause".into(),
                    });
                    f.seqs[seq].stmts[position] = f.stmts.alloc(Stmt::Block {
                        dst: None,
                        label,
                        body,
                    });
                }
                Stmt::If { then, else_, .. } => {
                    visit(types, f, then, fails, landings);
                    visit(types, f, else_, fails, landings);
                }
                Stmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(types, f, case.body, fails, landings);
                    }
                    if let Some(seq) = default {
                        visit(types, f, seq, fails, landings);
                    }
                }
                Stmt::Block { body, .. } | Stmt::Loop { body, .. } => {
                    visit(types, f, body, fails, landings)
                }
                Stmt::Protect { body, cleanup, .. } => {
                    visit(types, f, body, fails, landings);
                    visit(types, f, cleanup, false, landings);
                }
                _ => {}
            }
        }
    }
    let mut pending = Vec::new();
    for (id, f) in module.functions.iter_mut() {
        if let Some(body) = f.body {
            let mut landings = Vec::new();
            visit(&module.core.types, f, body, f.fails, &mut landings);
            pending.extend(landings.into_iter().map(|(seq, error)| (id, seq, error)));
        }
    }
    if pending.is_empty() {
        return Ok(());
    }
    // This backend-owned binding is introduced only after Core's monomorphic verification.
    // A local Break keeps normal Flow verification in force for the conservative fails edge.
    if module
        .functions
        .iter()
        .any(|(_, function)| function.name == INFALLIBLE_CAUSE_FAIL)
    {
        return Err(format!(
            "function name `{INFALLIBLE_CAUSE_FAIL}` is reserved for cause lowering"
        ));
    }
    let mut helper = d::Function::new(INFALLIBLE_CAUSE_FAIL);
    let parameter = helper.vars.alloc(d::VarData {
        hint: "error".into(),
        repr: d::Repr::Ref,
        ty: TypeId::ANY,
    });
    helper.params.push(parameter);
    helper.ret = TypeId::NEVER;
    helper.infallible_cause_fail = true;
    helper.external = Some(d::External {
        bindings: vec![d::ExternalBinding {
            target: INTERNAL_TARGET.into(),
            host: d::HostRef::Global(INFALLIBLE_CAUSE_FAIL.into()),
        }],
    });
    let helper = module.functions.alloc(helper);
    for (id, seq, error) in pending {
        let f = &mut module.functions[id];
        let call = f.stmts.alloc(Stmt::Let {
            dst: None,
            value: Value::Call {
                callee: d::Callee::Direct(helper),
                args: vec![error.into()],
                suspends: false,
                on_fail: None,
            },
            on_defect: None,
            on_suspend: None,
        });
        f.seqs[seq].stmts.push(call);
    }
    Ok(())
}

const INTERNAL_TARGET: &str = "emela_emb_internal";
pub(crate) const INFALLIBLE_CAUSE_FAIL: &str = "emela_infallible_cause_fail";
