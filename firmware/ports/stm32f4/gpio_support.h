/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_STM32F4_GPIO_SUPPORT_H
#define EMELINO_STM32F4_GPIO_SUPPORT_H
#include <stdint.h>
/* Internal CS operations use the same GPIO checks but preserve the peripheral owner. */
int32_t stm32f4_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial);
int32_t stm32f4_gpio_write_owned(uint16_t owner, int32_t pin, int32_t state);
void stm32f4_gpio_release_owned(uint16_t owner, int32_t pin);
#endif
