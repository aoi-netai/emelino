//! Shared generated runtime helpers.
use super::*;

pub(in crate::backend::emit) fn emit_int_to_string_helper(writer: &mut Writer) {
    writer.begin_block(
        "static inline EMELA_UNUSED EmelaString emela_emb_int_to_string(int64_t value, char *buffer, uint16_t capacity)",
    );
    writer.line("uint64_t magnitude = value < INT64_C(0) ? (uint64_t)(-(value + INT64_C(1))) + UINT64_C(1) : (uint64_t)value;");
    writer.line("uint32_t digit_count = UINT32_C(1);");
    writer.begin_block("for (uint64_t remaining = magnitude; remaining >= UINT64_C(10); remaining /= UINT64_C(10))");
    writer.line("++digit_count;");
    writer.end_block();
    writer
        .line("uint32_t length = digit_count + (value < INT64_C(0) ? UINT32_C(1) : UINT32_C(0));");
    writer.begin_block("if (length > (uint32_t)capacity)");
    writer.line(
        "emela_board_defect(EMELA_DEFECT_STRING_CAPACITY, \"string capacity exceeded\", UINT16_C(24));",
    );
    writer.end_block();
    writer.line("uint32_t cursor = length;");
    writer.begin_block("do");
    writer.line("buffer[--cursor] = (char)('0' + magnitude % UINT64_C(10));");
    writer.line("magnitude /= UINT64_C(10);");
    writer.end_block();
    writer.line("while (magnitude != UINT64_C(0));");
    writer.begin_block("if (value < INT64_C(0))");
    writer.line("buffer[0] = '-';");
    writer.end_block();
    writer.line("return (EmelaString){ .data = buffer, .length = (uint16_t)length };");
    writer.end_block();
    writer.blank();
}
