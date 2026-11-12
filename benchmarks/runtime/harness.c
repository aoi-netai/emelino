#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "emela_embedded.h"
#include "emela_board.h"

int32_t reference(int32_t n, int32_t seed);

_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t size) {
    fprintf(stderr, "defect %d: %.*s\n", (int)kind, (int)size, message);
    abort();
}

static uint64_t now_ns(void) {
    struct timespec time;
    if (clock_gettime(CLOCK_MONOTONIC, &time) != 0) abort();
    return (uint64_t)time.tv_sec * UINT64_C(1000000000) + (uint64_t)time.tv_nsec;
}

int main(int argc, char **argv) {
    if (argc != 5) return 2;
    int32_t n = (int32_t)strtol(argv[2], NULL, 10);
    int32_t seed = (int32_t)strtol(argv[3], NULL, 10);
    uint64_t iterations = strtoull(argv[4], NULL, 10);
    if (strcmp(argv[1], "check") == 0) {
        for (uint64_t i = 0; i < iterations; ++i) {
            int32_t input = (int32_t)((uint32_t)seed + (uint32_t)(i & 255));
            int32_t actual = bench(n, input), expected = reference(n, input);
            if (actual != expected) {
                fprintf(stderr, "n=%" PRId32 " seed=%" PRId32 ": got %" PRId32 " expected %" PRId32 "\n", n, input, actual, expected);
                return 1;
            }
        }
        puts("ok");
        return 0;
    }
#ifdef BENCH_AUDIT
    if (strcmp(argv[1], "audit") == 0) {
        void bench_audit_print(void);
        int32_t actual = bench(n, seed);
        if (actual != reference(n, seed)) return 1;
        bench_audit_print();
        return 0;
    }
#endif
    int32_t (*fn)(int32_t, int32_t);
    if (strcmp(argv[1], "generated") == 0) fn = bench;
    else if (strcmp(argv[1], "c") == 0) fn = reference;
    else return 2;
    uint64_t checksum = 0, start = now_ns();
    for (uint64_t i = 0; i < iterations; ++i)
        checksum += (uint32_t)fn(n, (int32_t)((uint32_t)seed + (uint32_t)(i & 255)));
    uint64_t elapsed = now_ns() - start;
    printf("{\"elapsed_ns\":%" PRIu64 ",\"checksum\":%" PRIu64 "}\n", elapsed, checksum);
    return 0;
}
