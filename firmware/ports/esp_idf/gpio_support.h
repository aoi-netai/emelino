/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_ESP_IDF_GPIO_SUPPORT_H
#define EMELINO_ESP_IDF_GPIO_SUPPORT_H
#include <stdint.h>
int32_t emelino_idf_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial);
int32_t emelino_idf_gpio_write_owned(uint16_t owner, int32_t pin, int32_t state);
void emelino_idf_gpio_release_owned(uint16_t owner, int32_t pin);
#endif
