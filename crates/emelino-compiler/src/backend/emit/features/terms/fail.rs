//! crate::contract::ir::Term::Fail
use super::super::*;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    term: &Term,
    _dst: Option<VarId>,
) -> Result<(), Vec<Error>> {
    let layouts = context.layouts;
    let function = context.function;
    let writer = &mut *context.writer;
    let names = context.names;
    let string_buffers = context.string_buffers;
    let loops = &mut *context.loops;
    let blocks = &mut *context.blocks;
    let protects = &mut *context.protects;
    let next_temp = &mut *context.next_temp;
    let next_label = &mut *context.next_label;
    let Term::Fail(error) = term else {
        unreachable!("terminal dispatch")
    };
    {
        if !function.fails {
            return Err(invalid_flow_ir(
                &function.name,
                "Fail appears in a function without a failure channel",
            ));
        }
        emit_error_assignment(layouts, function, error, "*emela_error_out", writer)?;
        emit_pending_cleanups(
            layouts,
            function,
            0,
            writer,
            names,
            string_buffers,
            loops,
            blocks,
            protects,
            next_temp,
            next_label,
        )?;
        emit_failure_return(layouts, function, writer);
    };
    Ok(())
}

pub(in crate::backend::emit) fn emit_function(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    term: &Term,
    _string_buffers: &StringBufferPlan,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    match term {
        Term::Fail(error) => {
            if !function.fails {
                return Err(invalid_flow_ir(
                    &function.name,
                    "Fail appears in a function without a failure channel",
                ));
            }
            emit_error_assignment(layouts, function, error, "*emela_error_out", writer)?;
            emit_failure_return(layouts, function, writer);
        }
        _ => unreachable!("function terminal"),
    };
    Ok(())
}
