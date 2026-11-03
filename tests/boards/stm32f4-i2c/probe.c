#include <stdint.h>
#include "stm32f4xx_hal.h"
volatile uint32_t i2c_test_stage;
volatile uint32_t i2c_test_command;
volatile uint32_t i2c_test_checks;
volatile uint32_t i2c_test_failures;
volatile uint32_t i2c_test_last_failure;
volatile uint32_t i2c_test_injected_nacks;
HAL_StatusTypeDef __real_HAL_I2C_Master_Seq_Transmit_IT(I2C_HandleTypeDef *, uint16_t, uint8_t *, uint16_t, uint32_t);
HAL_StatusTypeDef __wrap_HAL_I2C_Master_Seq_Transmit_IT(I2C_HandleTypeDef *handle, uint16_t address,
        uint8_t *data, uint16_t size, uint32_t options) {
    /* Renode's controller does not generate AF for an absent target. Test only
       the driver's NACK mapping/retry path using an explicit HAL completion. */
    if (address == (81U << 1)) {
        i2c_test_injected_nacks++;
        handle->ErrorCode = HAL_I2C_ERROR_AF;
        HAL_I2C_ErrorCallback(handle);
        return HAL_OK;
    }
    return __real_HAL_I2C_Master_Seq_Transmit_IT(handle, address, data, size, options);
}
void i2c_test_check(int32_t number, int32_t success) {
    i2c_test_checks++;
    if (!success) { i2c_test_failures++; i2c_test_last_failure = (uint32_t)number; }
}
void i2c_test_wait(int32_t stage) {
    i2c_test_stage = (uint32_t)stage;
    while (i2c_test_command != (uint32_t)stage) __asm volatile("nop");
}
