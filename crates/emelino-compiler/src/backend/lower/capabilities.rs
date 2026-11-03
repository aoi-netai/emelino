//! Reject unsupported operations before calling the corresponding stage interfaces.
use crate::contract::ir::*;
use crate::{Error, ErrorKind, Options};
use std::collections::HashSet;

pub(super) fn validate(program: &Program, options: &Options) -> Result<(), Vec<Error>> {
    fn require(function: &str, feature: &str) -> Result<(), Vec<Error>> {
        Err(vec![Error {
            kind: ErrorKind::Unsupported,
            message: format!("function `{function}`: {feature} is not supported by this backend"),
        }])
    }
    fn ty(
        program: &Program,
        function: &str,
        id: TypeId,
        seen: &mut HashSet<TypeId>,
    ) -> Result<(), Vec<Error>> {
        if !seen.insert(id) {
            return Ok(());
        }
        match program.core.ty(id) {
            Type::String | Type::Bytes | Type::Any => {}
            Type::Tuple(fields) => {
                for id in fields {
                    ty(program, function, *id, seen)?;
                }
            }
            Type::List(item) => {
                ty(program, function, *item, seen)?;
            }
            Type::Enum(id, args) => {
                for id in args {
                    ty(program, function, *id, seen)?;
                }
                for variant in &program.core.enums[*id].variants {
                    for field in &variant.tys {
                        ty(program, function, *field, seen)?;
                    }
                }
            }
            Type::Fn(_) | Type::Handler(_) | Type::Evidence | Type::Cell(_) => {}
            Type::FiberScope => require(function, "fiber scopes")?,
            _ => {}
        }
        Ok(())
    }
    if !options.task_adapters().is_empty() {
        require("options", "task adapters")?;
    }
    if !options.external_adapters().is_empty()
        || !options.isr_entries().is_empty()
        || !options.isr_safe_externals().is_empty()
    {
        require("options", "external adapters and ISR entries")?;
    }
    for (_, f) in program.functions.iter() {
        if f.vars
            .iter()
            .any(|(_, v)| matches!(program.core.ty(v.ty), Type::Cell(_)))
            || f.stmts.iter().any(|(_, s)| {
                matches!(
                    s,
                    Stmt::Let {
                        value: Value::Builtin {
                            op: Builtin::CellNew | Builtin::CellGet | Builtin::CellSet,
                            ..
                        },
                        ..
                    }
                )
            })
        {
            require(&f.name, "shared Cell values")?;
        }
        if f.stmts.iter().any(|(_, s)| {
            matches!(
                s,
                Stmt::Let {
                    value: Value::Fiber { .. },
                    ..
                }
            )
        }) {
            require(&f.name, "Cause operations")?;
        }
        if f.constant || f.canonical_const.is_some() {
            require(&f.name, "const definitions")?;
        }

        if f.suspends {
            require(&f.name, "suspension")?;
        }
        if f.external.is_some() && f.suspends {
            require(&f.name, "external C bindings")?;
        }

        let mut seen = HashSet::new();
        ty(program, &f.name, f.ret, &mut seen)?;
        for (id, var) in f.vars.iter() {
            if f.params.contains(&id)
                || f.stmts
                    .iter()
                    .any(|(_, stmt)| super::handler::statement_uses(stmt, id))
                || f.seqs
                    .iter()
                    .any(|(_, seq)| super::handler::term_uses(&seq.term, id))
            {
                ty(program, &f.name, var.ty, &mut seen)?;
            }
        }
        for (_, stmt) in f.stmts.iter() {
            match stmt {
                Stmt::Protect { .. } => {}
                Stmt::Let {
                    dst: _,
                    value,
                    on_defect: _,
                    on_suspend,
                } => {
                    if on_suspend.is_some() {
                        require(&f.name, "suspension points")?;
                    }
                    match value {
                        Value::Closure(_)
                        | Value::MakeClosure { .. }
                        | Value::FnRef(_)
                        | Value::EnvGet { .. }
                        | Value::EvLookup { .. }
                        | Value::EvExtend { .. }
                        | Value::Handler { .. }
                        | Value::IsError { .. }
                        | Value::AsError { .. } => {}
                        Value::Fiber { op, .. }
                            if !matches!(
                                op,
                                FiberOp::Attempt {
                                    suspends: false,
                                    ..
                                } | FiberOp::CauseKind { .. }
                                    | FiberOp::CauseToString
                                    | FiberOp::CauseRaise
                            ) =>
                        {
                            require(&f.name, "fiber operations")?
                        }
                        Value::Builtin {
                            op: Builtin::CellNew | Builtin::CellGet | Builtin::CellSet,
                            ..
                        } => {}
                        Value::Runtime { op, .. }
                            if !matches!(op, RuntimeOp::ShowString | RuntimeOp::DebugPrint) => {}
                        Value::Call {
                            callee: _,
                            suspends,
                            ..
                        } if *suspends => {
                            require(&f.name, "suspending calls")?;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        for (_, seq) in f.seqs.iter() {
            match seq.term {
                Term::Fail(_) => {}
                Term::Unwind => require(&f.name, "unwinding")?,
                _ => {}
            }
        }
    }
    Ok(())
}
