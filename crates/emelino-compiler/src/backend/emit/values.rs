//! Value expressions, projections, and generated-value stores.
use super::functions::emit_retain_operand;
use super::{
    StringBufferPlan, TypeLayouts,
    diagnostics::{invalid_flow_ir, unsupported},
    features,
};
use crate::contract::ir::{Builtin, Type, TypeId};
use crate::contract::ir::{Const, Function, Operand, PrimOp, PrimTy, RuntimeOp, VarId};
use crate::semantics::abi::LayoutRead;
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

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::emit) fn emit_aggregate_field_store(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    owner_ty: TypeId,
    variant: usize,
    field: usize,
    field_ty: TypeId,
    value: &Operand,
    target: &str,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    let expression = operand_expression(value).map_err(|kind| unsupported(&function.name, kind))?;
    if layouts.is_recursive_box_field(owner_ty, variant, field) {
        let box_kind = layouts.recursive_box_kind(field_ty).ok_or_else(|| {
            invalid_flow_ir(
                &function.name,
                "recursive aggregate field has no heap box layout",
            )
        })?;
        let field_type = layouts.c_type(field_ty);
        writer.assign(
            target,
            &format!("emela_heap_allocate(sizeof({field_type}), UINT32_C({box_kind}))"),
        );
        layouts.emit_owned_value_store(
            field_ty,
            &expression,
            &format!("*(({field_type} *){target})"),
            writer,
        );
    } else {
        if layouts.is_managed_value(field_ty) {
            emit_retain_operand(layouts, function, value, writer);
        }
        writer.assign(target, &expression);
    }
    Ok(())
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

use super::layouts::LayoutEmission;
