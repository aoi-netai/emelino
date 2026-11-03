//! Value expressions, projections, and generated-value stores.
use super::{StringBufferPlan, TypeLayouts, features};
use crate::contract::ir::{Builtin, Type};
use crate::contract::ir::{Const, Function, Operand, PrimOp, PrimTy, RuntimeOp, VarId};
use crate::{Error, writer::Writer};

pub(in crate::backend::emit) fn operand_is_string(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    operand: &Operand,
) -> bool {
    match operand {
        Operand::Const(Const::String(_)) => true,
        Operand::Var(variable) => {
            matches!(
                layouts.shapes.types[function.vars[*variable].ty],
                Type::String
            )
        }
        _ => false,
    }
}

pub(in crate::backend::emit) fn emit_builtin(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    op: Builtin,
    args: &[Operand],
    string_buffers: &StringBufferPlan,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    features::builtins::emit(
        &mut features::BuiltinContext {
            layouts,
            function,
            string_buffers,
            writer,
        },
        dst,
        op,
        args,
    )
}

pub(in crate::backend::emit) fn emit_runtime(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    op: RuntimeOp,
    args: &[Operand],
    string_buffers: &StringBufferPlan,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    features::runtime::emit(
        &mut features::BuiltinContext {
            layouts,
            function,
            string_buffers,
            writer,
        },
        dst,
        op,
        args,
    )
}

pub(in crate::backend::emit) fn primitive_expression(
    op: PrimOp,
    ty: PrimTy,
    args: &[Operand],
) -> Result<String, &'static str> {
    features::primitive_types::emit(op, ty, args)
}

pub(in crate::backend::emit) fn operand_expression(
    operand: &Operand,
) -> Result<String, &'static str> {
    features::operands::dispatch(operand)
}

pub(in crate::backend::emit) fn constant_expression(
    value: &crate::contract::ir::Const,
) -> Result<Option<String>, &'static str> {
    features::constants::dispatch(value)
}
