/* Shared observations only; no peripheral access or injected responses. */
#ifndef EMELINO_TEST_PROBE_H
#define EMELINO_TEST_PROBE_H
#include <stdint.h>

/* Prefixes keep each scenario's debugger-visible metrics independent. */
#define EMELINO_TEST_PROBE(prefix) \
    volatile uint32_t prefix##_stage; \
    volatile uint32_t prefix##_command; \
    volatile uint32_t prefix##_checks; \
    volatile uint32_t prefix##_failures; \
    volatile uint32_t prefix##_last_failure; \
    void prefix##_check(int32_t number, int32_t success) { \
        ++prefix##_checks; \
        if (!success) { \
            ++prefix##_failures; \
            prefix##_last_failure = (uint32_t)number; \
        } \
    } \
    void prefix##_wait(int32_t stage) { \
        prefix##_stage = (uint32_t)stage; \
        while (prefix##_command != (uint32_t)stage) {} \
    }
#endif
