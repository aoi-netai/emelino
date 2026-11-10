#include <stdint.h>
#include "hardware/sync.h"
volatile uint32_t pico_test_checks, pico_test_failed, pico_test_last_failed;
volatile uint32_t pico_test_failure_ids[16];
volatile uint32_t pico_test_done, pico_test_stage, pico_test_continue;
volatile uint32_t pico_test_callbacks, pico_test_isr;
void pico_check(int32_t number, int32_t success) {
    pico_test_checks++;
    if (!success) {
        if (pico_test_failed < 16) pico_test_failure_ids[pico_test_failed] = number;
        pico_test_failed++; pico_test_last_failed = number;
    }
}
void pico_wait(int32_t stage) {
    pico_test_stage = stage;
    while (pico_test_continue < (uint32_t)stage) __asm volatile("nop");
    /* Allow pending IRQs to execute before the next observation. */
    for (volatile unsigned i = 0; i < 64; i++) __asm volatile("nop");
}
void pico_finish(void) { pico_test_done = 1; }
void pico_callback(void) {
    uint32_t ipsr;
    __asm volatile("mrs %0, ipsr" : "=r"(ipsr));
    pico_test_isr = ipsr != 0;
    pico_test_callbacks++;
}
int32_t pico_callback_count(void) { return pico_test_callbacks; }
