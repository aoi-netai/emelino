use super::*;
pub(in crate::backend::emit) mod attempt;
pub(in crate::backend::emit) mod cause_kind;
pub(in crate::backend::emit) mod cause_raise;
pub(in crate::backend::emit) mod cause_to_string;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
) -> Result<(), Vec<Error>> {
    let Value::Fiber { op, .. } = value else {
        unreachable!("Fiber dispatch")
    };
    match op {
        crate::contract::ir::FiberOp::Scope { .. } => {
            Err(unsupported(&context.function.name, "scope"))
        }
        crate::contract::ir::FiberOp::Fork => Err(unsupported(&context.function.name, "fork")),
        crate::contract::ir::FiberOp::Deferred => {
            Err(unsupported(&context.function.name, "deferred"))
        }
        crate::contract::ir::FiberOp::Join => Err(unsupported(&context.function.name, "join")),
        crate::contract::ir::FiberOp::Await { .. } => {
            Err(unsupported(&context.function.name, "await"))
        }
        crate::contract::ir::FiberOp::Interrupt => {
            Err(unsupported(&context.function.name, "interrupt"))
        }
        crate::contract::ir::FiberOp::Attempt { .. } => attempt::emit(context, dst, value),
        crate::contract::ir::FiberOp::Complete => {
            Err(unsupported(&context.function.name, "complete"))
        }
        crate::contract::ir::FiberOp::DeferredAwait => {
            Err(unsupported(&context.function.name, "deferred_await"))
        }
        crate::contract::ir::FiberOp::Poll => Err(unsupported(&context.function.name, "poll")),
        crate::contract::ir::FiberOp::CauseKind { .. } => cause_kind::emit(context, dst, value),
        crate::contract::ir::FiberOp::CauseToString => cause_to_string::emit(context, dst, value),
        crate::contract::ir::FiberOp::CauseRaise => cause_raise::emit(context, dst, value),
    }
}
