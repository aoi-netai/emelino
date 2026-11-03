/* SPDX-License-Identifier: Apache-2.0 */
#include <stdint.h>

#include "bsp.h"
#include "registers.h"

volatile uint32_t spi_test_stage;
volatile uint32_t spi_test_command;
volatile uint32_t spi_test_checks;
volatile uint32_t spi_test_failures;
volatile uint32_t spi_test_last_failure;
volatile uint32_t spi_test_injection;
volatile uint32_t spi_test_injected_timeouts;
volatile uint32_t spi_test_injected_cleanup_errors;

void spi_test_check(int32_t number, int32_t success) {
    spi_test_checks++;
    if (!success) {
        spi_test_failures++;
        spi_test_last_failure = (uint32_t)number;
    }
}

int32_t spi_test_control(void) { return (int32_t)SPI1->CR1; }
int64_t spi_test_time(void) { return (int64_t)stm32f4_now_us(); }
void spi_test_inject(int32_t mode) { spi_test_injection = (uint32_t)mode; }

void spi_test_wait(int32_t stage) {
    spi_test_stage = (uint32_t)stage;
    while (spi_test_command != (uint32_t)stage) __asm volatile("nop");
}

/* Linker wrapping injects device faults only in this test binary. */
HAL_StatusTypeDef __real_HAL_SPI_TransmitReceive(SPI_HandleTypeDef *, const uint8_t *, uint8_t *, uint16_t, uint32_t);
HAL_StatusTypeDef __wrap_HAL_SPI_TransmitReceive(SPI_HandleTypeDef *handle, const uint8_t *tx,
                                                uint8_t *rx, uint16_t size, uint32_t timeout) {
    if (spi_test_injection == 1) {
        spi_test_injected_timeouts++;
        handle->ErrorCode = HAL_SPI_ERROR_NONE;
        return HAL_TIMEOUT;
    }
    return __real_HAL_SPI_TransmitReceive(handle, tx, rx, size, timeout);
}

int32_t __real_stm32f4_gpio_write_owned(uint16_t, int32_t, int32_t);
int32_t __wrap_stm32f4_gpio_write_owned(uint16_t owner, int32_t pin, int32_t state) {
    if (spi_test_injection != 0 && pin == 4 && state == 1) {
        spi_test_injected_cleanup_errors++;
        return 6; /* GpioErrorKind.InvalidState + 1 */
    }
    return __real_stm32f4_gpio_write_owned(owner, pin, state);
}
