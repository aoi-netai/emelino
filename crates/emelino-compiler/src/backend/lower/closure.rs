//! Closure conversion on this compiler's owned contract IR.
//!
//! Adapted from the fixed upstream lower/closure.rs; see docs/migration-source.md.
//! Reachability, lifted bodies, and capture metadata are constructed inside lower.
use super::positions as function_positions;
use std::collections::HashSet;

use crate::contract::ir::Type;
use crate::contract::ir::{Callee, FnId, Function, Program as Module, Stmt, Term, Value};

pub(super) fn runtime_reachable_without_initializers(
    program: &Module,
    options: &crate::Options,
    resolved_constants: &HashSet<FnId>,
) -> HashSet<FnId> {
    let mut pending: Vec<_> = program
        .functions
        .iter()
        .filter_map(|(id, f)| (f.exported || f.name == options.entry()).then_some(id))
        .collect();
    let mut reachable = HashSet::new();
    loop {
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if resolved_constants.contains(&id) {
                continue;
            }
            let function = &program.functions[id];
            let (positions, _) = function_positions(function);
            for (stmt, value) in function.stmts.iter() {
                if !positions.contains_key(&stmt) {
                    continue;
                }
                if let Stmt::Let {
                    value:
                        Value::FnRef(target)
                        | Value::MakeClosure {
                            function: target, ..
                        }
                        | Value::Call {
                            callee: crate::contract::ir::Callee::Direct(target),
                            ..
                        },
                    ..
                } = value
                {
                    pending.push(*target);
                }
            }
        }
        // Cause rendering and suspended operation diagnostics invoke error Show
        // functions through generated C, rather than an explicit IR Call.
        let shows_causes = reachable.iter().any(|id| {
            let function = &program.functions[*id];
            let (positions, _) = function_positions(function);
            function.stmts.iter().any(|(id, statement)| {
                positions.contains_key(&id)
                    && match statement {
                        Stmt::Let {
                            value: Value::Fiber { .. },
                            ..
                        } => true,
                        Stmt::Let {
                            value:
                                Value::Call {
                                    callee: Callee::Op { effect, op, .. },
                                    ..
                                },
                            ..
                        } => program.core.effects[*effect].ops[*op].suspend,
                        _ => false,
                    }
            })
        });
        if shows_causes
            && reachable
                .iter()
                .any(|id| needs_error_abi(program, &program.functions[*id]))
        {
            for &(error, show) in &program.error_shows {
                if !reachable.contains(&show)
                    && reachable.iter().any(|id| {
                        program.functions[*id].vars.iter().any(|(_, var)| {
                        matches!(program.core.types[var.ty], Type::Enum(id, _) if id == error)
                    })
                    })
                {
                    pending.push(show);
                }
            }
        }
        if pending.is_empty() {
            break;
        }
    }
    reachable
}

fn needs_error_abi(program: &Module, function: &Function) -> bool {
    fn failing_type(
        program: &Module,
        ty: crate::contract::ir::TypeId,
        seen: &mut HashSet<crate::contract::ir::TypeId>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match program.core.ty(ty) {
            Type::Fn(ft) => {
                ft.fails
                    || ft
                        .params
                        .iter()
                        .chain([&ft.ret])
                        .any(|&ty| failing_type(program, ty, seen))
            }
            Type::Tuple(fields) | Type::Enum(_, fields) => {
                fields.iter().any(|&ty| failing_type(program, ty, seen))
                    || match program.core.ty(ty) {
                        Type::Enum(id, _) => program.core.enums[*id]
                            .variants
                            .iter()
                            .flat_map(|variant| &variant.tys)
                            .any(|&ty| failing_type(program, ty, seen)),
                        _ => false,
                    }
            }
            Type::List(value) | Type::Cell(value) => failing_type(program, *value, seen),
            Type::Handler(effect) => program.core.effects[*effect].ops.iter().any(|operation| {
                operation.fails
                    || operation
                        .params
                        .iter()
                        .chain([&operation.ret])
                        .any(|&ty| failing_type(program, ty, seen))
            }),
            _ => false,
        }
    }
    function.fails
        || function
            .seqs
            .iter()
            .any(|(_, sequence)| matches!(sequence.term, Term::Fail(_)))
        || function.stmts.iter().any(|(_, statement)| {
            matches!(
                statement,
                Stmt::Let {
                    value: Value::Call {
                        on_fail: Some(_),
                        ..
                    } | Value::Fiber {
                        on_fail: Some(_),
                        ..
                    } | Value::IsError { .. }
                        | Value::AsError { .. },
                    ..
                }
            )
        })
        || std::iter::once(function.ret)
            .chain(function.vars.iter().map(|(_, var)| var.ty))
            .any(|ty| failing_type(program, ty, &mut HashSet::new()))
}
