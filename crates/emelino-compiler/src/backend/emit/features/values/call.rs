//! crate::contract::ir::Value::Call and its three Callee entry points.
use super::super::*;
pub(in crate::backend::emit) fn emit_direct(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
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
    match value {
        Value::Call {
            callee: Callee::Direct(id),
            args,
            suspends: false,
            on_fail,
        } => {
            let result_ty = layouts.functions.function_results[id];
            let name = names
                .get(id)
                .ok_or_else(|| unsupported(&function.name, "call to missing function"))?;
            let mut args = args
                .iter()
                .map(operand_expression)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|variant| unsupported(&function.name, variant))?;
            let mut output_scope = false;
            let mut string_output_args = None;
            if string_buffers.has_string_output_call(*id) {
                if let Some(dst) = dst {
                    let Some((buffer, capacity)) = string_buffers.buffer(*dst) else {
                        return Err(invalid_flow_ir(
                            &function.name,
                            "String-returning call has no planned output buffer",
                        ));
                    };
                    string_output_args = Some((buffer, capacity));
                } else {
                    let suffix = *next_temp;
                    *next_temp += 1;
                    let buffer = format!("emela_call_string_discard_{suffix}");
                    writer.begin_block("");
                    writer.line(&format!("char {buffer}[{}];", string_buffers.capacity));
                    string_output_args =
                        Some((buffer, format!("UINT16_C({})", string_buffers.capacity)));
                    output_scope = true;
                }
            }
            if let Some(edge) = on_fail {
                let error_name = format!("emela_call_error_{}", *next_temp);
                *next_temp += 1;
                writer.begin_block("");
                writer.decl("EmelaError", &error_name, Some("{0}"));
                args.push(format!("&{error_name}"));
                if let Some((buffer, capacity)) = string_output_args {
                    args.push(buffer);
                    args.push(capacity);
                }
                let expression = format!("{name}({})", args.join(", "));
                let discarded =
                    emit_call_result(layouts, result_ty, *dst, &expression, writer, next_temp);
                writer.begin_block(&format!("if ({error_name}.tag != UINT32_C(0))"));
                emit_call_failure_edge(
                    layouts,
                    function,
                    *edge,
                    &error_name,
                    writer,
                    names,
                    string_buffers,
                    loops,
                    blocks,
                    protects,
                    next_temp,
                    next_label,
                )?;
                writer.end_block();
                if let Some(value) = discarded {
                    layouts.emit_release_value_expression(result_ty, &value, writer);
                }
                writer.end_block();
            } else {
                if let Some((buffer, capacity)) = string_output_args {
                    args.push(buffer);
                    args.push(capacity);
                }
                let expression = format!("{name}({})", args.join(", "));
                if let Some(value) =
                    emit_call_result(layouts, result_ty, *dst, &expression, writer, next_temp)
                {
                    layouts.emit_release_value_expression(result_ty, &value, writer);
                }
            }
            if output_scope {
                writer.end_block();
            }
        }
        other => return Err(unsupported(&function.name, value_name(other))),
    };
    Ok(())
}
pub(in crate::backend::emit) fn emit_indirect(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
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
    match value {
        Value::Call {
            callee: Callee::Indirect(callee),
            args,
            suspends: false,
            on_fail,
        } => {
            emit_indirect_call(
                layouts,
                function,
                *dst,
                *callee,
                args,
                *on_fail,
                writer,
                names,
                string_buffers,
                loops,
                blocks,
                protects,
                next_temp,
                next_label,
            )?;
        }
        other => return Err(unsupported(&function.name, value_name(other))),
    };
    Ok(())
}
pub(in crate::backend::emit) fn emit_op(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
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
    match value {
        Value::Call {
            callee:
                Callee::Op {
                    handler,
                    effect,
                    op,
                },
            args,
            on_fail,
            ..
        } if layouts.shapes.core.effects[*effect].ops[*op].suspend => {
            writer.begin_block("");
            if on_fail.is_some() {
                writer.line("EmelaError operation_error = {0};");
            }
            Err(unsupported(&function.name, "suspending operations"))?;
            if let Some(edge) = on_fail {
                writer.begin_block("if (operation_error.tag != 0)");
                emit_call_failure_edge(
                    layouts,
                    function,
                    *edge,
                    "operation_error",
                    writer,
                    names,
                    string_buffers,
                    loops,
                    blocks,
                    protects,
                    next_temp,
                    next_label,
                )?;
                writer.end_block();
            }
            writer.end_block();
        }
        Value::Call {
            callee:
                Callee::Op {
                    handler,
                    effect,
                    op,
                },
            args,
            suspends: false,
            on_fail,
        } => {
            emit_handler_operation_call(
                layouts,
                function,
                *dst,
                *handler,
                *effect,
                *op,
                args,
                *on_fail,
                writer,
                names,
                string_buffers,
                loops,
                blocks,
                protects,
                next_temp,
                next_label,
            )?;
        }
        other => return Err(unsupported(&function.name, value_name(other))),
    };
    Ok(())
}
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
) -> Result<(), Vec<Error>> {
    let Value::Call { callee, .. } = value else {
        unreachable!("Call entry")
    };
    match callee {
        Callee::Direct(_) => emit_direct(context, dst, value),
        Callee::Indirect(_) => emit_indirect(context, dst, value),
        Callee::Op { .. } => emit_op(context, dst, value),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::emit) fn emit_indirect_call(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    callee: VarId,
    args: &[Operand],
    on_fail: Option<OnFail>,
    writer: &mut Writer,
    names: &HashMap<FnId, String>,
    string_buffers: &StringBufferPlan,
    loops: &mut Vec<Vec<VarId>>,
    blocks: &mut Vec<BlockTarget>,
    protects: &mut Vec<ProtectTarget>,
    next_temp: &mut usize,
    next_label: &mut usize,
) -> Result<(), Vec<Error>> {
    emit_function_value_call(
        layouts,
        function,
        dst,
        function.vars[callee].ty,
        &variable_name(callee),
        args,
        on_fail,
        writer,
        names,
        string_buffers,
        loops,
        blocks,
        protects,
        next_temp,
        next_label,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::emit) fn emit_function_value_call(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    function_type_id: TypeId,
    callee_expression: &str,
    args: &[Operand],
    on_fail: Option<OnFail>,
    writer: &mut Writer,
    names: &HashMap<FnId, String>,
    string_buffers: &StringBufferPlan,
    loops: &mut Vec<Vec<VarId>>,
    blocks: &mut Vec<BlockTarget>,
    protects: &mut Vec<ProtectTarget>,
    next_temp: &mut usize,
    next_label: &mut usize,
) -> Result<(), Vec<Error>> {
    let Type::Fn(function_type) = layouts.shapes.types[function_type_id].clone() else {
        return Err(invalid_flow_ir(
            &function.name,
            "indirect call target is not a function value",
        ));
    };
    if function_type.suspends {
        return Err(unsupported(&function.name, "indirect call with suspend"));
    }
    if function_type.fails != on_fail.is_some() {
        return Err(invalid_flow_ir(
            &function.name,
            "indirect call failure edge does not match its function type",
        ));
    }
    let mut parameter_types: Vec<TypeId> = function_type
        .params
        .into_iter()
        .filter(|&parameter| layouts.type_repr(parameter) != Repr::Void)
        .collect();
    if function_type.ev {
        parameter_types.push(TypeId::EVIDENCE);
    }
    if args.len() != parameter_types.len() {
        return Err(invalid_flow_ir(
            &function.name,
            &format!(
                "indirect call has {} arguments, expected {}",
                args.len(),
                parameter_types.len()
            ),
        ));
    }

    let mut call_args = Vec::new();
    for (&parameter_type, argument) in parameter_types.iter().zip(args) {
        if operand_type(function, argument) != Some(parameter_type) {
            return Err(invalid_flow_ir(
                &function.name,
                "indirect call argument has the wrong type",
            ));
        }
        call_args.push(
            operand_expression(argument).map_err(|variant| unsupported(&function.name, variant))?,
        );
    }

    match dst {
        None => {}
        Some(_) if layouts.type_repr(function_type.ret) == Repr::Void => {
            return Err(invalid_flow_ir(
                &function.name,
                "void indirect call has a result destination",
            ));
        }
        Some(result) if function.vars[result].ty != function_type.ret => {
            return Err(invalid_flow_ir(
                &function.name,
                "indirect call result has the wrong type",
            ));
        }
        Some(_) => {}
    }

    let callee_name = callee_expression;
    writer.begin_block(&format!("if ({callee_name}.call == NULL)"));
    writer.line("emela_board_defect(EMELA_DEFECT_PANIC, \"null function value\", UINT16_C(19));");
    writer.end_block();
    if let Some(edge) = on_fail {
        let error_name = format!("emela_indirect_error_{}", *next_temp);
        *next_temp += 1;
        writer.begin_block("");
        writer.decl("EmelaError", &error_name, Some("{0}"));
        call_args.push(format!("&{error_name}"));
        call_args.insert(0, format!("{callee_name}.env"));
        let expression = format!("{callee_name}.call({})", call_args.join(", "));
        let discarded = emit_call_result(
            layouts,
            function_type.ret,
            dst,
            &expression,
            writer,
            next_temp,
        );
        writer.begin_block(&format!("if ({error_name}.tag != UINT32_C(0))"));
        emit_call_failure_edge(
            layouts,
            function,
            edge,
            &error_name,
            writer,
            names,
            string_buffers,
            loops,
            blocks,
            protects,
            next_temp,
            next_label,
        )?;
        writer.end_block();
        if let Some(value) = discarded {
            layouts.emit_release_value_expression(function_type.ret, &value, writer);
        }
        writer.end_block();
    } else {
        call_args.insert(0, format!("{callee_name}.env"));
        let expression = format!("{callee_name}.call({})", call_args.join(", "));
        if let Some(value) = emit_call_result(
            layouts,
            function_type.ret,
            dst,
            &expression,
            writer,
            next_temp,
        ) {
            layouts.emit_release_value_expression(function_type.ret, &value, writer);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::emit) fn emit_handler_operation_call(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    dst: Option<VarId>,
    handler: VarId,
    effect: EffectId,
    operation: usize,
    args: &[Operand],
    on_fail: Option<OnFail>,
    writer: &mut Writer,
    names: &HashMap<FnId, String>,
    string_buffers: &StringBufferPlan,
    loops: &mut Vec<Vec<VarId>>,
    blocks: &mut Vec<BlockTarget>,
    protects: &mut Vec<ProtectTarget>,
    next_temp: &mut usize,
    next_label: &mut usize,
) -> Result<(), Vec<Error>> {
    if layouts.shapes.types[function.vars[handler].ty] != Type::Handler(effect) {
        return Err(invalid_flow_ir(
            &function.name,
            "operation call handler type does not match its effect",
        ));
    }
    let Some(layout_index) = layouts.shapes.handler_layout_ids.get(&effect).copied() else {
        return Err(invalid_flow_ir(
            &function.name,
            "dynamic operation call has no handler layout",
        ));
    };
    let Some(&function_type) = layouts.shapes.handler_layouts[layout_index]
        .operations
        .get(operation)
    else {
        return Err(invalid_flow_ir(
            &function.name,
            "dynamic operation index is outside its handler layout",
        ));
    };
    let handler_expression = variable_name(handler);
    let handler_type = crate::semantics::abi::handler_type_name(effect);
    let handler_pointer = format!("(({handler_type} *){handler_expression})");
    writer.begin_block(&format!("if ({handler_expression} == NULL)"));
    writer
        .line("emela_board_defect(EMELA_DEFECT_PANIC, \"invalid effect handler\", UINT16_C(22));");
    writer.end_block();
    writer.begin_block(&format!("if (!{handler_pointer}->active)"));
    writer.line(&format!(
        "emela_board_defect(EMELA_DEFECT_PANIC, {handler_pointer}->released_message, {handler_pointer}->released_message_length);"
    ));
    writer.end_block();

    emit_function_value_call(
        layouts,
        function,
        dst,
        function_type,
        &format!("({handler_pointer}->op_{operation})"),
        args,
        on_fail,
        writer,
        names,
        string_buffers,
        loops,
        blocks,
        protects,
        next_temp,
        next_label,
    )
}

pub(in crate::backend::emit) fn emit_call_result(
    layouts: &TypeLayouts<'_>,
    result: TypeId,
    dst: Option<VarId>,
    expression: &str,
    writer: &mut Writer,
    next_temp: &mut usize,
) -> Option<String> {
    if let Some(dst) = dst {
        writer.assign(&variable_name(dst), expression);
    } else if layouts.is_managed_value(result) {
        let discarded = format!("emela_discarded_call_{}", *next_temp);
        *next_temp += 1;
        writer.decl(&layouts.c_type(result), &discarded, Some(expression));
        return Some(discarded);
    } else {
        writer.line(&format!("{expression};"));
    }
    None
}
