//! Contiguous immutable arrays. Stores acquire element ownership before releasing old values.
use super::super::*;

pub(in crate::backend::emit) fn helpers(layouts: &TypeLayouts<'_>, writer: &mut Writer) {
    if layouts.shapes.array_types.is_empty() {
        return;
    }
    writer.line(include_str!("../../runtime/array_bounds.c"));
    for &ty in &layouts.shapes.array_types {
        let Type::Array(element) = layouts.shapes.types[ty] else {
            unreachable!()
        };
        let name = layouts.array_name(ty);
        let storage = layouts.cell_content_c_type(element);
        writer.begin_block(&format!(
            "static inline EMELA_UNUSED {name} *{name}_new(uint32_t length)"
        ));
        writer.line("if (length == 0) return NULL;");
        writer.line(&format!("if (length > INT32_MAX || (size_t)length > (SIZE_MAX - sizeof({name}))/sizeof({storage})) emela_board_defect(EMELA_DEFECT_HEAP_EXHAUSTED, \"Array length exceeds capacity\", UINT16_C(29));"));
        writer.line(&format!("{name} *result = emela_heap_allocate(sizeof({name}) + (size_t)length * sizeof({storage}), UINT32_C({}));", layouts.array_kind(ty)));
        writer.line("result->length = length; return result;");
        writer.end_block();
        writer.begin_block(&format!("static inline EMELA_UNUSED {name} *{name}_copy(const {name} *input, uint32_t from, uint32_t length)"));
        writer.line(&format!("{name} *result = {name}_new(length);"));
        writer.begin_block("for (uint32_t i = 0; i < length; ++i)");
        if layouts.type_repr(element) != Repr::Void {
            layouts.emit_owned_value_store(
                element,
                "input->values[from + i]",
                "result->values[i]",
                writer,
            );
        } else {
            writer.line("(void)input; (void)from;");
        }
        writer.end_block();
        writer.line("return result;");
        writer.end_block();
    }
}

pub(in crate::backend::emit) fn emit(
    context: &mut BuiltinContext<'_, '_>,
    dst: Option<VarId>,
    op: Builtin,
    args: &[Operand],
) -> Emission {
    let layouts = context.layouts;
    let function = context.function;
    let dst =
        dst.ok_or_else(|| invalid_flow_ir(&function.name, "Array operation has no destination"))?;
    let first = args
        .first()
        .and_then(|a| operand_type(function, a))
        .ok_or_else(|| invalid_flow_ir(&function.name, "Array operation needs a typed input"))?;
    let array_ty = match op {
        Builtin::ArrayFromList => function.vars[dst].ty,
        Builtin::ArrayConcat => match layouts.shapes.types[first] {
            Type::List(t) => t,
            _ => return Err(invalid_flow_ir(&function.name, "Array.concat input")),
        },
        _ => first,
    };
    let Type::Array(element) = layouts.shapes.types[array_ty] else {
        return Err(invalid_flow_ir(&function.name, "Array operation type"));
    };
    let expressions = args
        .iter()
        .map(operand_expression)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|v| unsupported(&function.name, v))?;
    let name = layouts.array_name(array_ty);
    let out = variable_name(dst);
    let writer = &mut *context.writer;
    writer.begin_block("");
    match op {
        Builtin::ArraySize => writer.assign(
            &out,
            &format!(
                "{0} == NULL ? 0 : (int32_t)(({name} *){0})->length",
                expressions[0]
            ),
        ),
        Builtin::ArrayGet => {
            let none =
                option_variant_expression(layouts, function, function.vars[dst].ty, "None", None)?;
            let payload = if layouts.type_repr(element) == Repr::Void {
                None
            } else {
                Some("input->values[index]")
            };
            let some = option_variant_expression(
                layouts,
                function,
                function.vars[dst].ty,
                "Some",
                payload,
            )?;
            writer.line(&format!(
                "const {name} *input = {}; int32_t index = {};",
                expressions[0], expressions[1]
            ));
            writer
                .begin_block("if (input == NULL || index < 0 || (uint32_t)index >= input->length)");
            writer.assign(&out, &none);
            writer.end_block();
            writer.begin_block("else");
            writer.assign(&out, &some);
            layouts.emit_own_value_in_place(function.vars[dst].ty, &out, writer);
            writer.end_block();
        }
        Builtin::ArrayFromList | Builtin::ArrayConcat => {
            let node = layouts.list_node_name(first);
            writer.line(&format!(
                "const {node} *head = {}; uint32_t length = 0;",
                expressions[0]
            ));
            writer.begin_block(&format!(
                "for (const {node} *node = head; node != NULL; node = node->tail)"
            ));
            let count = if op == Builtin::ArrayConcat {
                format!("(node->head == NULL ? 0 : (({name} *)node->head)->length)")
            } else {
                "1".into()
            };
            writer.line(&format!("uint32_t count = {count};"));
            writer.line("if (count > (uint32_t)INT32_MAX - length) emela_board_defect(EMELA_DEFECT_HEAP_EXHAUSTED, \"Array length exceeds Int\", UINT16_C(24));");
            writer.line("length += count;");
            writer.end_block();
            writer.line(&format!(
                "{name} *result = {name}_new(length); uint32_t i = 0;"
            ));
            writer.begin_block(&format!(
                "for (const {node} *node = head; node != NULL; node = node->tail)"
            ));
            if op == Builtin::ArrayConcat {
                writer.line(&format!("const {name} *part = node->head;"));
                writer.begin_block("for (uint32_t j = 0; part != NULL && j < part->length; ++j)");
                if layouts.type_repr(element) != Repr::Void {
                    layouts.emit_owned_value_store(
                        element,
                        "part->values[j]",
                        "result->values[i]",
                        writer,
                    );
                }
                writer.line("++i;");
                writer.end_block();
            } else {
                if layouts.type_repr(element) != Repr::Void {
                    layouts.emit_owned_value_store(
                        element,
                        "node->head",
                        "result->values[i]",
                        writer,
                    );
                }
                writer.line("++i;");
            }
            writer.end_block();
            writer.line("(void)i;");
            writer.assign(&out, "result");
        }
        Builtin::ArrayToList => {
            let node = layouts.list_node_name(function.vars[dst].ty);
            let kind = layouts.list_index(function.vars[dst].ty);
            writer.line(&format!(
                "const {name} *input = {}; void *head = NULL;",
                expressions[0]
            ));
            writer.begin_block("for (uint32_t i = input == NULL ? 0 : input->length; i != 0; --i)");
            writer.line(&format!(
                "{node} *node = emela_heap_allocate(sizeof({node}), UINT32_C({kind}));"
            ));
            if layouts.type_repr(element) != Repr::Void {
                layouts.emit_owned_value_store(
                    element,
                    "input->values[i - 1]",
                    "node->head",
                    writer,
                );
            }
            writer.line("node->tail = head; head = node;");
            writer.end_block();
            writer.assign(&out, "head");
        }
        Builtin::ArraySlice => {
            writer.line(&format!(
                "const {name} *input = {}; int32_t from = {}, to = {};",
                expressions[0], expressions[1], expressions[2]
            ));
            writer.line("uint32_t length = input == NULL ? 0 : input->length;");
            writer.line("uint32_t start = from < 0 ? 0 : (uint32_t)from > length ? length : (uint32_t)from;");
            writer
                .line("uint32_t end = to < 0 ? 0 : (uint32_t)to > length ? length : (uint32_t)to;");
            writer.assign(
                &out,
                &format!("{name}_copy(input, start, start < end ? end - start : 0)"),
            );
        }
        Builtin::ArraySet => {
            writer.line(&format!(
                "{name} *input = {}; int32_t index = {};",
                expressions[0], expressions[1]
            ));
            writer.line("uint32_t length = input == NULL ? 0 : input->length;");
            writer.line("if (index < 0 || (uint32_t)index >= length) emela_array_set_bounds(index, length);");
            let reusable = if let Operand::Var(source) = args[0] {
                crate::backend::array_liveness::can_update(
                    function,
                    dst,
                    source,
                    &layouts.functions.closure_functions,
                    &layouts.functions.closure_environments,
                )
            } else {
                false
            };
            let loan = match args[0] {
                Operand::Var(v) => crate::backend::array_liveness::loop_loan(function, v),
                _ => None,
            };
            let unique = if let Some(p) = loan {
                format!(
                    "(references == 1 || (references == 2 && input == emela_array_loan_{}))",
                    u32::from(p.into_raw())
                )
            } else {
                "references == 1".into()
            };
            writer.line("uint32_t references = ((emela_heap_block *)((unsigned char *)input - emela_heap_header_size()))->references;");
            writer.line(&format!("{name} *result;"));
            writer.begin_block(&format!("if ({} && {unique})", u8::from(reusable)));
            writer.line("result = input; emela_heap_retain(result);");
            writer.end_block();
            writer.begin_block("else");
            writer.line(&format!("result = {name}_copy(input, 0, length);"));
            writer.end_block();
            if layouts.type_repr(element) != Repr::Void {
                writer.decl(&layouts.c_type(element), "new_value", Some(&expressions[2]));
                layouts.emit_own_value_in_place(element, "new_value", writer);
                writer.decl(
                    &layouts.c_type(element),
                    "old_value",
                    Some("result->values[index]"),
                );
                writer.line("(void)old_value;");
                writer.assign("result->values[index]", "new_value");
                layouts.emit_release_value_expression(element, "old_value", writer);
            }
            writer.assign(&out, "result");
        }
        _ => unreachable!("Array dispatch"),
    }
    writer.end_block();
    Ok(())
}
