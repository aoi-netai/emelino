/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"

/* The initial board profile deliberately keeps reset HSI and all APB clocks at 16 MHz. */
uint32_t SystemCoreClock = 16000000U;
static volatile uint64_t milliseconds;

uint32_t HAL_RCC_GetHCLKFreq(void) { return SystemCoreClock; }
uint32_t HAL_RCC_GetPCLK1Freq(void) { return SystemCoreClock; }
uint32_t HAL_RCC_GetPCLK2Freq(void) { return SystemCoreClock; }
uint32_t HAL_GetTick(void) { return (uint32_t)milliseconds; }

void SysTick_Handler(void) { ++milliseconds; }

void stm32f4_time_init(void) {
    milliseconds = 0;
    (void)SysTick_Config(SystemCoreClock / 1000U);
    NVIC_SetPriority(SysTick_IRQn, 15);
    /* Avoid reporting a nearly complete millisecond before the initial reload. */
    while (SysTick->VAL == 0) {}
}

uint64_t stm32f4_now_us(void) {
    uint32_t previous = stm32f4_interrupt_lock();
    uint64_t whole = milliseconds;
    uint32_t ticks = SysTick->VAL;
    /* Account for the single wrap pending while IRQs are masked. */
    if (SCB->ICSR & SCB_ICSR_PENDSTSET_Msk) {
        ++whole;
        ticks = SysTick->VAL;
    }
    uint32_t reload = SysTick->LOAD;
    stm32f4_interrupt_unlock(previous);
    return whole * 1000U + (reload - ticks) / (SystemCoreClock / 1000000U);
}

bool stm32f4_delay_us(uint64_t delay, uint64_t deadline) {
    uint64_t start = stm32f4_now_us();
    if (start >= deadline || delay > deadline - start) return false;
    for (;;) {
        /* Check the deadline even when an IRQ delayed the final sample past
           both the requested interval and the caller's absolute deadline. */
        uint64_t now = stm32f4_now_us();
        if (now >= deadline) return false;
        if (now - start >= delay) return true;
    }
}
