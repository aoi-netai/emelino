/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_PICO_SDK_BSP_H
#define EMELINO_PICO_SDK_BSP_H
#include <stdbool.h>
#include <stdint.h>
#include "hardware/sync.h"
#include "pico/platform.h"

enum {
    EMELINO_PICO_OWNER_GPIO = 1,
    EMELINO_PICO_OWNER_UART0 = 2,
    EMELINO_PICO_OWNER_SPI0 = 3,
    EMELINO_PICO_OWNER_I2C0 = 4,
    EMELINO_PICO_OWNER_SPI_CS_BASE = 16,
};

/* This port runs on core 0. Nested critical sections preserve the caller's mask. */
static inline uint32_t emelino_pico_interrupt_lock(void) {
    return save_and_disable_interrupts();
}
static inline void emelino_pico_interrupt_unlock(uint32_t previous) {
    restore_interrupts(previous);
}
static inline uint32_t emelino_pico_exception_number(void) {
    return __get_current_exception();
}

bool emelino_pico_pin_valid(int32_t pin);
uint16_t emelino_pico_pin_owner(int32_t pin);
/* GPIO status: zero succeeds, 1 invalid argument, 5 resource conflict. */
int32_t emelino_pico_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count);
void emelino_pico_pins_release(uint16_t owner, const int32_t *pins, uint16_t count);
void emelino_pico_time_init(void);
uint64_t emelino_pico_now_us(void);
bool emelino_pico_delay_us(uint64_t delay, uint64_t deadline);
void emela_board_init(void);

#define emelino_platform_lock emelino_pico_interrupt_lock
#define emelino_platform_unlock emelino_pico_interrupt_unlock
#define emelino_platform_time_init emelino_pico_time_init
#endif
