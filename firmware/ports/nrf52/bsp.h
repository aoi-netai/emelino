/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_NRF52_BSP_H
#define EMELINO_NRF52_BSP_H
#include <stdbool.h>
#include <stdint.h>
#include "nrf.h"

enum {
    NRF52_OWNER_GPIO = 1,
    NRF52_OWNER_UART0 = 2,
    NRF52_OWNER_SPI2 = 3,
    NRF52_OWNER_TWI0 = 4,
    NRF52_OWNER_SPI_CS_BASE = 16,
};

/* Critical sections nest and restore their caller's interrupt mask. */
static inline uint32_t nrf52_interrupt_lock(void) {
    uint32_t previous = __get_PRIMASK();
    __disable_irq();
    return previous;
}
static inline void nrf52_interrupt_unlock(uint32_t previous) { __set_PRIMASK(previous); }
static inline uint32_t nrf52_exception_number(void) { return __get_IPSR(); }

bool nrf52_pin_valid(int32_t pin);
uint16_t nrf52_pin_owner(int32_t pin);
/* GPIO status: zero succeeds, 1 invalid argument, 5 resource conflict. */
int32_t nrf52_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count);
void nrf52_pins_release(uint16_t owner, const int32_t *pins, uint16_t count);
/* Caller validates pin/port before using register helpers. */
NRF_GPIO_Type *nrf52_gpio_port(uint32_t port);
void nrf52_time_init(void);
uint64_t nrf52_now_us(void);
bool nrf52_delay_us(uint64_t delay, uint64_t deadline);
void emela_board_init(void);
#define emelino_platform_lock nrf52_interrupt_lock
#define emelino_platform_unlock nrf52_interrupt_unlock
#define emelino_platform_time_init nrf52_time_init
#endif
