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
            Type::Any => require(function, "owned values")?,
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
            Type::Fn(_) | Type::Handler(_) | Type::Evidence | Type::Cell(_) => {
                require(function, "functions, effects and Cell")?
            }
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
        if f.constant || f.canonical_const.is_some() {
            require(&f.name, "const definitions")?;
        }
        if f.fails || f.env.is_some() {
            require(&f.name, "failure and environment ABI")?;
        }
        if f.suspends {
            require(&f.name, "suspension")?;
        }
        if f.external.is_some() && f.suspends {
            require(&f.name, "external C bindings")?;
        }
        if f.derivation.is_some() {
            require(&f.name, "derived operations")?;
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
                Stmt::Protect { .. } => require(&f.name, "protected cleanup")?,
                Stmt::Let {
                    dst,
                    value,
                    on_defect,
                    on_suspend,
                } => {
                    if on_defect.is_some() {
                        require(&f.name, "defect recovery")?;
                    }
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
                        | Value::AsError { .. } => require(&f.name, "closure and effect values")?,
                        Value::Fiber { .. } => require(&f.name, "fiber operations")?,
                        Value::Builtin { .. } => require(&f.name, "builtin operations")?,
                        Value::Runtime { .. } => require(&f.name, "runtime operations")?,
                        Value::Coerce(source)
                            if dst.is_some_and(|dst| {
                                f.vars[dst].ty == TypeId::ANY && f.vars[*source].ty == TypeId::UNIT
                            }) =>
                        {
                            require(&f.name, "Unit in Any")?
                        }
                        Value::Call {
                            callee,
                            suspends,
                            on_fail,
                            ..
                        } => {
                            if !matches!(callee, Callee::Direct(_)) || on_fail.is_some() {
                                require(&f.name, "indirect and fallible calls")?;
                            }
                            if *suspends {
                                require(&f.name, "suspending calls")?;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        for (_, seq) in f.seqs.iter() {
            match seq.term {
                Term::Fail(_) => require(&f.name, "failure and defect paths")?,
                Term::Unwind => require(&f.name, "unwinding")?,
                _ => {}
            }
        }
    }
    Ok(())
}
