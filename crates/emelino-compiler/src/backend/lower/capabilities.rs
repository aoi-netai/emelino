//! Reject features whose implementation has not been introduced yet.
use crate::contract::ir::Program;
use crate::{Error, ErrorKind, Options};
pub(super) fn validate(program: &Program, options: &Options) -> Result<(), Vec<Error>> {
    if !options.external_adapters().is_empty()
        || !options.isr_entries().is_empty()
        || !options.isr_safe_externals().is_empty()
    {
        return Err(vec![Error {
            kind: ErrorKind::Unsupported,
            message: "external adapters and ISR entries are not supported by this backend".into(),
        }]);
    }
    for (_, function) in program.functions.iter() {
        for (_, statement) in function.stmts.iter() {
            if let crate::contract::ir::Stmt::Let {
                value: crate::contract::ir::Value::Fiber { op, .. },
                ..
            } = statement
            {
                if !matches!(
                    op,
                    crate::contract::ir::FiberOp::Scope { .. }
                        | crate::contract::ir::FiberOp::Fork
                        | crate::contract::ir::FiberOp::Deferred
                        | crate::contract::ir::FiberOp::Attempt {
                            suspends: false,
                            ..
                        }
                        | crate::contract::ir::FiberOp::CauseKind { .. }
                        | crate::contract::ir::FiberOp::CauseToString
                        | crate::contract::ir::FiberOp::CauseRaise
                ) {
                    return Err(vec![Error {
                        kind: ErrorKind::Unsupported,
                        message: format!(
                            "function `{}`: Fiber operation {:?} is not supported by this backend",
                            function.name, op
                        ),
                    }]);
                }
            }
        }

        if function.constant || function.canonical_const.is_some() {
            return Err(vec![Error {
                kind: ErrorKind::Unsupported,
                message: format!(
                    "function `{}`: const definitions are not supported by this backend",
                    function.name
                ),
            }]);
        }
    }
    Ok(())
}
