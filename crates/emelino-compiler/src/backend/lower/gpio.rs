//! Elide the stateless standard GPIO handler when its only uses are operations.
//!
//! In particular an ISR must not allocate a handler merely to call a checked
//! GPIO read/write/toggle. Inspect the factory's complete executed body before
//! removing it: state, captures, init/release, aliases and escaping handlers keep
//! the ordinary handler path and its existing safety checks.
use super::*;
use crate::contract::lowered::ClosureMetadata;

pub(super) fn lower(program: &mut Program, closures: &ClosureMetadata) {
    let Some((factory, effect, operations)) = factory(program, closures) else {
        return;
    };
    for (_, function) in program.functions.iter_mut() {
        let (active, _) = positions(function);
        let candidates: Vec<_> = function
            .stmts
            .iter()
            .filter_map(|(statement, value)| {
                let Stmt::Let {
                    dst: Some(handler),
                    value:
                        Value::Call {
                            callee: Callee::Direct(target),
                            args,
                            suspends: false,
                            on_fail: None,
                        },
                    on_defect: None,
                    on_suspend: None,
                } = value
                else {
                    return None;
                };
                if *target != factory
                    || !args.is_empty()
                    || !active.contains_key(&statement)
                    || function.params.contains(handler)
                    || function.stmts.iter().any(|(_, statement)| {
                        matches!(statement, Stmt::Loop { vars, .. } if vars.contains(handler))
                    })
                    || function
                        .stmts
                        .iter()
                        .filter(|(_, statement)| statement.dst() == Some(*handler))
                        .count()
                        != 1
                    || function
                        .seqs
                        .iter()
                        .any(|(_, seq)| super::handler::term_uses(&seq.term, *handler))
                {
                    return None;
                }
                let safe_uses = function.stmts.iter().all(|(_, statement)| {
                    if !super::handler::statement_uses(statement, *handler) {
                        return true;
                    }
                    matches!(statement, Stmt::Let { value: Value::Call {
                    callee: Callee::Op { handler: used, effect: used_effect, op }, args,
                    suspends: false, ..
                }, on_suspend: None, .. } if used == handler && *used_effect == effect
                    && *op < operations.len()
                    && !args.iter().any(|arg| matches!(arg, Operand::Var(var) if var == handler)))
                });
                safe_uses.then_some((statement, *handler))
            })
            .collect();
        for (definition, handler) in candidates {
            for (_, statement) in function.stmts.iter_mut() {
                if let Stmt::Let {
                    value:
                        Value::Call {
                            callee:
                                Callee::Op {
                                    handler: used,
                                    effect: used_effect,
                                    op,
                                },
                            ..
                        },
                    ..
                } = statement
                {
                    if *used == handler && *used_effect == effect {
                        let target = operations[*op];
                        if let Stmt::Let {
                            value: Value::Call { callee, .. },
                            ..
                        } = statement
                        {
                            *callee = Callee::Direct(target);
                        }
                    }
                }
            }
            function.stmts[definition] = Stmt::Let {
                dst: None,
                value: Value::Const(Const::Unit),
                on_defect: None,
                on_suspend: None,
            };
        }
    }
}

fn factory(program: &Program, closures: &ClosureMetadata) -> Option<(FnId, EffectId, Vec<FnId>)> {
    let (factory, function) = program
        .functions
        .iter()
        .find(|(_, function)| function.name == "Embedded.Board.BoardGpio")?;
    if !function.params.is_empty()
        || function.fails
        || function.suspends
        || function.external.is_some()
    {
        return None;
    }
    let Type::Handler(effect) = program.core.types[function.ret] else {
        return None;
    };
    // Keep the saved legacy ABI's handler lowering unchanged.
    if !program.core.effects[effect].ops.iter().any(|operation| {
        operation.name == "enable_interrupt"
            && operation.params.len() == 4
            && matches!(program.core.types[operation.params[3]], Type::Fn(_))
    }) {
        return None;
    }
    let sequence = &function.seqs[function.body?];
    let Term::Return(Some(Operand::Var(returned))) = sequence.term else {
        return None;
    };
    let mut references = HashMap::new();
    let mut operations = None;
    for &statement in &sequence.stmts {
        match &function.stmts[statement] {
            Stmt::Let {
                dst: Some(variable),
                value: Value::FnRef(target),
                on_defect: None,
                on_suspend: None,
            } if !closures.functions.contains_key(target) => {
                references.insert(*variable, *target);
            }
            Stmt::Let {
                dst: Some(variable),
                value:
                    Value::Handler {
                        effect: value_effect,
                        ops,
                        ..
                    },
                on_defect: None,
                on_suspend: None,
            } if *variable == returned && *value_effect == effect => {
                operations = Some(
                    ops.iter()
                        .map(|op| references.get(op).copied())
                        .collect::<Option<Vec<_>>>()?,
                );
            }
            _ => return None,
        }
    }
    Some((factory, effect, operations?))
}
