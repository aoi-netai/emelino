//! Bounded marshalling for SPI transactions. The BSP never owns Emela lists.
use super::*;

pub(super) fn emit(
    op: &Operation,
    layouts: &TypeLayouts<'_>,
    function: &Function,
    variables: &[String],
    capacity: u16,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    writer.decl(&layouts.c_type(function.ret), "result", Some("{0}"));
    match op.name.as_str() {
        "create_device" => {
            writer.line("EmelaSpiDevice device = {0};");
            writer.line(&format!(
                "EmelaSpiTransactionStatus status = emela_spi_create_device({}, &device);",
                to_board(layouts, op.params[0], &variables[0])?
            ));
            copy_status(writer);
            writer.line("if (result.f0 == 0 && result.f1 == 0 && result.f2 == 0) result.f3.payload.v0.f0 = device.slot;");
        }
        "recover" => {
            writer.line(&format!(
                "EmelaSpiTransactionStatus status = emela_spi_recover({});",
                variables[0]
            ));
            copy_status(writer);
        }
        "transaction" => transaction(op, layouts, variables, capacity, writer)?,
        _ => unreachable!(),
    }
    writer.ret(Some("result"));
    Ok(())
}

fn copy_status(writer: &mut Writer) {
    writer.line("result.f0 = status.primary_spi;");
    writer.line("result.f1 = status.primary_gpio;");
    writer.line("result.f2 = status.cleanup_gpio;");
}

fn transaction(
    op: &Operation,
    layouts: &TypeLayouts<'_>,
    variables: &[String],
    capacity: u16,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    let Type::List(operation_type) = layouts.shapes.types[op.params[1]] else {
        return Err(unsupported(
            "Embedded.Board.spi_transaction",
            "invalid SPI operation list",
        ));
    };
    let Type::Enum(operation_id, _) = layouts.shapes.types[operation_type] else {
        unreachable!()
    };
    let words_type = layouts.shapes.core.enums[operation_id].variants[0].tys[0];
    let operations_node = layouts.list_node_name(op.params[1]);
    let words_node = layouts.list_node_name(words_type);
    writer.line(&format!(
        "EmelaSpiOperation operations[{capacity}] = {{0}};"
    ));
    writer.line(&format!("uint16_t inputs[{capacity}] = {{0}};"));
    writer.line(&format!("uint16_t outputs[{capacity}] = {{0}};"));
    writer.line(&format!("uint16_t lengths[{capacity}] = {{0}};"));
    writer.line("uint32_t count = 0, input_count = 0, expected_output = 0;");
    writer.begin_block(&format!("for (const {operations_node} *cursor = (const {operations_node} *){}; cursor != NULL; cursor = (const {operations_node} *)cursor->tail)", variables[1]));
    writer.line(&format!("if (count == UINT32_C({capacity})) {{ result.f0 = EMELA_SPI_ERROR_KIND_CAPACITY_EXCEEDED + 1; return result; }}"));
    writer.line("EmelaSpiOperation *operation = &operations[count++];");
    writer.line("operation->tag = cursor->head.tag;");
    writer.begin_block("if (operation->tag == EMELA_SPI_OPERATION_READ)");
    writer.line("int32_t size = cursor->head.payload.v1.f0;");
    writer.line(
        "if (size < 0) { result.f0 = EMELA_SPI_ERROR_KIND_INVALID_ARGUMENT + 1; return result; }",
    );
    writer.line(&format!("if ((uint32_t)size > UINT32_C({capacity})) {{ result.f0 = EMELA_SPI_ERROR_KIND_CAPACITY_EXCEEDED + 1; return result; }}"));
    writer.line("operation->length = (uint16_t)size;");
    writer.end_block();
    writer.begin_block("else if (operation->tag == EMELA_SPI_OPERATION_WRITE || operation->tag == EMELA_SPI_OPERATION_TRANSFER)");
    writer.line(&format!("const {words_node} *word = (const {words_node} *)(operation->tag == EMELA_SPI_OPERATION_WRITE ? cursor->head.payload.v0.f0 : cursor->head.payload.v2.f0);"));
    writer.line("operation->data = &inputs[input_count];");
    writer.begin_block(&format!(
        "for (; word != NULL; word = (const {words_node} *)word->tail)"
    ));
    writer.line(&format!("if (input_count == UINT32_C({capacity})) {{ result.f0 = EMELA_SPI_ERROR_KIND_CAPACITY_EXCEEDED + 1; return result; }}"));
    writer.line("if (word->head < 0 || word->head > INT32_C(65535)) { result.f0 = EMELA_SPI_ERROR_KIND_INVALID_ARGUMENT + 1; return result; }");
    writer.line("inputs[input_count++] = (uint16_t)word->head;");
    writer.line("++operation->length;");
    writer.end_block();
    writer.end_block();
    writer.begin_block("else");
    writer.line("result.f0 = EMELA_SPI_ERROR_KIND_INVALID_ARGUMENT + 1;");
    writer.ret(Some("result"));
    writer.end_block();
    writer.line(
        "if (operation->tag != EMELA_SPI_OPERATION_WRITE) expected_output += operation->length;",
    );
    writer.line(&format!("if (expected_output > UINT32_C({capacity})) {{ result.f0 = EMELA_SPI_ERROR_KIND_CAPACITY_EXCEEDED + 1; return result; }}"));
    writer.end_block();
    writer.line(&format!("EmelaSpiTransactionStatus status = emela_spi_transaction({}, operations, (uint16_t)count, {}, outputs, UINT16_C({capacity}), lengths);",
        to_board(layouts, op.params[0], &variables[0])?, variables[2]));
    copy_status(writer);
    writer.line("if (result.f0 != 0 || result.f1 != 0 || result.f2 != 0) return result;");
    // Check the entire response before allocating any lists, including exact
    // lengths for each read/transfer and the empty result of each write.
    writer.begin_block("for (uint32_t i = 0; i < count; ++i)");
    writer.line("uint16_t expected = operations[i].tag == EMELA_SPI_OPERATION_WRITE ? 0 : operations[i].length;");
    writer.line("if (lengths[i] != expected) { result.f0 = EMELA_SPI_ERROR_KIND_OTHER + 1; return result; }");
    writer.end_block();
    let Type::List(inner_type) = layouts.shapes.types[op.ret] else {
        unreachable!()
    };
    let inner_node = layouts.list_node_name(inner_type);
    let outer_node = layouts.list_node_name(op.ret);
    let inner_kind = layouts.list_index(inner_type);
    let outer_kind = layouts.list_index(op.ret);
    writer.line("uint32_t end = expected_output;");
    writer.begin_block("for (uint32_t i = count; i-- > 0;)");
    writer.line("void *inner = NULL;");
    writer.begin_block("for (uint16_t j = lengths[i]; j-- > 0;)");
    writer.line(&format!("{inner_node} *node = ({inner_node} *)emela_heap_allocate(sizeof({inner_node}), UINT32_C({inner_kind}));"));
    writer.line("node->head = (int32_t)outputs[--end];");
    writer.line("node->tail = inner;");
    writer.line("inner = node;");
    writer.end_block();
    writer.line(&format!("{outer_node} *node = ({outer_node} *)emela_heap_allocate(sizeof({outer_node}), UINT32_C({outer_kind}));"));
    writer.line("node->head = inner;");
    writer.line("node->tail = result.f3;");
    writer.line("result.f3 = node;");
    writer.end_block();
    Ok(())
}
