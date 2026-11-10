/* The signed index includes INT32_MIN; render through an unsigned magnitude. */
static inline EMELA_UNUSED void emela_array_set_bounds(int32_t index, uint32_t size) {
    char message[96];
    char digits[10];
    uint16_t length = 0;
    const char *prefix = "Array.set was given position ";
    while (*prefix) message[length++] = *prefix++;
    uint32_t magnitude = index < 0 ? UINT32_C(0) - (uint32_t)index : (uint32_t)index;
    if (index < 0) message[length++] = '-';
    unsigned count = 0;
    do { digits[count++] = (char)('0' + magnitude % 10); magnitude /= 10; } while (magnitude);
    while (count) message[length++] = digits[--count];
    prefix = " in an array of size ";
    while (*prefix) message[length++] = *prefix++;
    do { digits[count++] = (char)('0' + size % 10); size /= 10; } while (size);
    while (count) message[length++] = digits[--count];
    emela_board_defect(EMELA_DEFECT_PANIC, message, length);
}
