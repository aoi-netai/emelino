/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_STM32F4_BSP_H
#define EMELINO_STM32F4_BSP_H
#include <stdbool.h>
#include <stdint.h>
#include "registers.h"

enum {
    STM32F4_OWNER_GPIO = 1,
    STM32F4_OWNER_USART2 = 2,
    STM32F4_OWNER_SPI1 = 3,
    STM32F4_OWNER_I2C1 = 4,
    STM32F4_OWNER_SPI_CS_BASE = 16,
};
/* GPIO status: 0 success, 1 invalid argument, 5 resource conflict. */
bool stm32f4_pin_valid(int32_t pin);
uint16_t stm32f4_pin_owner(int32_t pin);
int32_t stm32f4_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count);
void stm32f4_pins_release(uint16_t owner, const int32_t *pins, uint16_t count);
/* Internal helpers; callers must first validate/claim their pin numbers. */
GPIO_TypeDef *stm32f4_gpio_port(uint32_t port);
void stm32f4_enable_gpio_clock(uint32_t port);

/* Optional board init.c hook, called after clock/time setup and before Emela. */
void emela_board_init(void);

void stm32f4_time_init(void);
uint64_t stm32f4_now_us(void);
bool stm32f4_delay_us(uint64_t delay, uint64_t deadline);
#endif
