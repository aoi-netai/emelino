//! crate::contract::ir::FiberOp::Attempt
use super::super::*;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
) -> Result<(), Vec<Error>> {
    let layouts = context.layouts;
    let function = context.function;
    let writer = &mut *context.writer;
    let Value::Fiber { args, .. } = value else {
        unreachable!("Fiber entry")
    };
    let dst = *dst;
    cause::emit_attempt_call(layouts, function, dst, args, writer)
}
