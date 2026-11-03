/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_STM32F4_REGISTERS_H
#define EMELINO_STM32F4_REGISTERS_H

#include "stm32f4xx_hal.h"

/* These critical sections nest and restore the caller's original PRIMASK. */
static inline uint32_t stm32f4_interrupt_lock(void) {
    uint32_t previous = __get_PRIMASK();
    __disable_irq();
    return previous;
}

static inline void stm32f4_interrupt_unlock(uint32_t previous) {
    __set_PRIMASK(previous);
}

static inline uint32_t stm32f4_exception_number(void) {
    return __get_IPSR();
}

#endif
