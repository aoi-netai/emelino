/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_ESP_IDF_BSP_H
#define EMELINO_ESP_IDF_BSP_H
#include <stdbool.h>
#include <stdint.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

enum { EMELINO_IDF_OWNER_GPIO = 1, EMELINO_IDF_OWNER_UART0,
       EMELINO_IDF_OWNER_SPI2, EMELINO_IDF_OWNER_I2C0,
       EMELINO_IDF_OWNER_CS_BASE = 16 };

/* ESP32-C3 is single-core. These short, nestable sections never block. */
static inline uint32_t emelino_idf_lock(void) {
    return (uint32_t)portSET_INTERRUPT_MASK_FROM_ISR();
}
static inline void emelino_idf_unlock(uint32_t previous) {
    portCLEAR_INTERRUPT_MASK_FROM_ISR(previous);
}
static inline bool emelino_idf_task_context(void) { return !xPortInIsrContext(); }
bool emelino_idf_pin_valid(int32_t pin);
uint16_t emelino_idf_pin_owner(int32_t pin);
/* GPIO status codes: 1 invalid argument, 5 resource conflict. */
int32_t emelino_idf_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count);
void emelino_idf_pins_release(uint16_t owner, const int32_t *pins, uint16_t count);
uint64_t emelino_idf_now_us(void);
bool emelino_idf_delay_us(uint64_t delay, uint64_t deadline);
bool emelino_idf_timeout_valid(int64_t timeout);
int emelino_idf_remaining_ms(uint64_t deadline);
void emelino_idf_poll_wait(uint64_t deadline);
void emela_board_init(void);
#endif
