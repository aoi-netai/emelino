/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "hardware/timer.h"

/* The SDK starts the hardware timer before main; no SysTick or alarm is needed. */
void emelino_pico_time_init(void) {}

uint64_t emelino_pico_now_us(void) {
    return time_us_64();
}

bool emelino_pico_delay_us(uint64_t delay, uint64_t deadline) {
    uint64_t start = emelino_pico_now_us();
    if (start >= deadline || delay > deadline - start) return false;
    for (;;) {
        uint64_t now = emelino_pico_now_us();
        if (now >= deadline) return false;
        if (now - start >= delay) return true;
        tight_loop_contents();
    }
}
