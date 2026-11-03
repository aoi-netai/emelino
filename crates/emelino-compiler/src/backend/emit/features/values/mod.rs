// Keep analysis dispatch exhaustive: every new variant needs an explicit planning decision.
use super::*;
pub(in crate::backend::emit) mod builtin;
pub(in crate::backend::emit) mod call;
pub(in crate::backend::emit) mod const_;
pub(in crate::backend::emit) mod field;
pub(in crate::backend::emit) mod list;
pub(in crate::backend::emit) mod list_head;
pub(in crate::backend::emit) mod list_is_empty;
pub(in crate::backend::emit) mod list_tail;
pub(in crate::backend::emit) mod make;
pub(in crate::backend::emit) mod prim;
pub(in crate::backend::emit) mod runtime;
pub(in crate::backend::emit) mod tag;
pub(in crate::backend::emit) mod tuple;
pub(in crate::backend::emit) mod tuple_get;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
) -> Result<(), Vec<Error>> {
    match value {
        Value::Const(_) => const_::emit(context, dst, value),
        Value::Prim { .. } => prim::emit(context, dst, value),
        Value::Call { .. } => call::emit(context, dst, value),
        Value::Builtin { .. } => builtin::emit(context, dst, value),
        Value::Make { .. } => make::emit(context, dst, value),
        Value::Field { .. } => field::emit(context, dst, value),
        Value::Tag(_) => tag::emit(context, dst, value),
        Value::Tuple(_) => tuple::emit(context, dst, value),
        Value::TupleGet { .. } => tuple_get::emit(context, dst, value),
        Value::List { .. } => list::emit(context, dst, value),
        Value::ListIsEmpty(_) => list_is_empty::emit(context, dst, value),
        Value::ListHead(_) => list_head::emit(context, dst, value),
        Value::ListTail(_) => list_tail::emit(context, dst, value),
        Value::FnRef(_) => Err(unsupported(&context.function.name, "fn_ref")),
        Value::Closure(_) => Err(unsupported(&context.function.name, "closure")),
        Value::MakeClosure { .. } => Err(unsupported(&context.function.name, "make_closure")),
        Value::EnvGet { .. } => Err(unsupported(&context.function.name, "env_get")),
        Value::EvLookup { .. } => Err(unsupported(&context.function.name, "ev_lookup")),
        Value::EvExtend { .. } => Err(unsupported(&context.function.name, "ev_extend")),
        Value::Handler { .. } => Err(unsupported(&context.function.name, "handler")),
        Value::Fiber { .. } => Err(unsupported(&context.function.name, "fiber")),
        Value::IsError { .. } => Err(unsupported(&context.function.name, "is_error")),
        Value::AsError { .. } => Err(unsupported(&context.function.name, "as_error")),
        Value::Runtime { .. } => runtime::emit(context, dst, value),
        Value::Coerce(_) => Err(unsupported(&context.function.name, "coerce")),
    }
}
