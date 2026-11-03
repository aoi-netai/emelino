/* SPDX-License-Identifier: Apache-2.0 */
#ifndef EMELINO_STM32F4_I2C_EVENTS_H
#define EMELINO_STM32F4_I2C_EVENTS_H

/* Include after the SDK types and the BSP critical-section helpers. */
static inline void stm32f4_i2c_event(I2C_HandleTypeDef *handle, volatile bool *stop_deferred) {
    /* HAL's final RXNE path waits for STOP to clear before reading DR. Its
       internal fixed 5 ms spin cannot share our deadline, especially in IRQ
       context. Preserve that ordering but let main perform the waiting. */
    if (handle->State == HAL_I2C_STATE_BUSY_RX && handle->XferCount == 1
        && (handle->Instance->CR1 & I2C_CR1_STOP)) {
        __HAL_I2C_DISABLE_IT(handle, I2C_IT_EVT | I2C_IT_BUF);
        *stop_deferred = true;
        return;
    }
    HAL_I2C_EV_IRQHandler(handle);
}

static inline void stm32f4_i2c_resume_event(I2C_HandleTypeDef *handle, volatile bool *stop_deferred) {
    uint32_t previous = stm32f4_interrupt_lock();
    if (*stop_deferred && !(handle->Instance->CR1 & I2C_CR1_STOP)) {
        *stop_deferred = false;
        if (handle->State == HAL_I2C_STATE_BUSY_RX)
            __HAL_I2C_ENABLE_IT(handle, I2C_IT_EVT | I2C_IT_BUF);
    }
    stm32f4_interrupt_unlock(previous);
}

#endif
