/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"

static volatile uint64_t milliseconds;

void SysTick_Handler(void) { ++milliseconds; }

void nrf52_time_init(void) {
    milliseconds = 0;
    (void)SysTick_Config(EMELA_BOARD_CLOCK_HZ / 1000U);
    NVIC_SetPriority(SysTick_IRQn, 7);
    while (SysTick->VAL == 0) {}
}

uint64_t nrf52_now_us(void) {
    uint32_t previous = nrf52_interrupt_lock();
    uint64_t whole = milliseconds;
    uint32_t ticks = SysTick->VAL;
    if (SCB->ICSR & SCB_ICSR_PENDSTSET_Msk) {
        ++whole;
        ticks = SysTick->VAL;
    }
    uint32_t reload = SysTick->LOAD;
    nrf52_interrupt_unlock(previous);
    return whole * 1000U + (reload - ticks) / (EMELA_BOARD_CLOCK_HZ / 1000000U);
}

bool nrf52_delay_us(uint64_t delay, uint64_t deadline) {
    uint64_t start = nrf52_now_us();
    if (start >= deadline || delay > deadline - start) return false;
    for (;;) {
        uint64_t now = nrf52_now_us();
        if (now >= deadline) return false;
        if (now - start >= delay) return true;
    }
}
