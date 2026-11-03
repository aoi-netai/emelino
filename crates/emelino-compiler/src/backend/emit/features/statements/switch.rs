//! crate::contract::ir::Stmt::Switch
use super::super::*;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    statement: &Stmt,
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
    match statement {
        Stmt::Switch {
            dst,
            on,
            cases,
            default,
        } => emit_switch(
            layouts,
            function,
            *dst,
            *on,
            cases,
            *default,
            writer,
            names,
            string_buffers,
            loops,
            blocks,
            protects,
            next_temp,
            next_label,
        ),
        other => Err(unsupported(&function.name, statement_name(other))),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::emit) fn emit_switch(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    on: SwitchOn,
    cases: &[crate::contract::ir::Case],
    default: Option<crate::contract::ir::SeqId>,
    writer: &mut Writer,
    names: &HashMap<FnId, String>,
    string_buffers: &StringBufferPlan,
    loops: &mut Vec<Vec<VarId>>,
    blocks: &mut Vec<BlockTarget>,
    protects: &mut Vec<ProtectTarget>,
    next_temp: &mut usize,
    next_label: &mut usize,
) -> Result<(), Vec<Error>> {
    let (scrutinee, tag_switch) = features::switches::scrutinee(layouts, function, on)?;

    if !tag_switch && default.is_none() {
        return Err(invalid_flow_ir(
            &function.name,
            "literal switch has no default sequence",
        ));
    }

    writer.begin_block(&format!("switch ({scrutinee})"));
    for case in cases {
        let label = features::case_keys::label(layouts, function, &case.key, on)?;
        writer.begin_block(&format!("case {label}:"));
        emit_compound_sequence(
            layouts,
            function,
            case.body,
            dst,
            writer,
            names,
            string_buffers,
            loops,
            blocks,
            protects,
            next_temp,
            next_label,
        )?;
        writer.line("break;");
        writer.end_block();
    }

    if let Some(default) = default {
        writer.begin_block("default:");
        emit_compound_sequence(
            layouts,
            function,
            default,
            dst,
            writer,
            names,
            string_buffers,
            loops,
            blocks,
            protects,
            next_temp,
            next_label,
        )?;
        writer.line("break;");
        writer.end_block();
    } else {
        writer.begin_block("default:");
        writer.line("emela_board_defect(EMELA_DEFECT_NO_MATCH, \"no match\", UINT16_C(8));");
        writer.end_block();
    }
    writer.end_block();
    Ok(())
}
