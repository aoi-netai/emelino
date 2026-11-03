/* SPDX-License-Identifier: Apache-2.0 */
#include <stdbool.h>
#include <stdint.h>
#include "board_config.h"
#include "bsp.h"
#include "emela_i2c.h"
#include "registers.h"
#include "i2c_events.h"

enum { OK = 0, INVALID_ARGUMENT = 1, CAPACITY_EXCEEDED = 2, NOT_CONFIGURED = 3,
       UNSUPPORTED = 4, TIMEOUT = 5, BUSY = 6, RESOURCE_CONFLICT = 7,
       INVALID_STATE = 8, NACK = 9, BUS_ERROR = 10, ARBITRATION_LOST = 11, OTHER = 12 };
static I2C_HandleTypeDef controller;
static bool configured;
static volatile bool occupied;
static bool interrupts_enabled;
static volatile bool phase_done;
static volatile uint32_t phase_error;
static volatile bool stop_deferred;
static uint8_t bytes[EMELA_PERIPHERAL_BUFFER_CAPACITY];
static const int32_t pins[] = {EMELA_I2C1_SCL_PIN, EMELA_I2C1_SDA_PIN};

static GPIO_TypeDef *pin_port(int32_t pin) {
    return stm32f4_gpio_port((uint32_t)pin / 16);
}
static uint16_t pin_bit(int32_t pin) { return (uint16_t)(1U << (pin % 16)); }
static bool expired(uint64_t deadline) { return stm32f4_now_us() >= deadline; }
static int32_t valid_id(EmelaI2cId id) {
    if (id < EMELA_I2C_ID_I2C1 || id > EMELA_I2C_ID_I2C3) return INVALID_ARGUMENT;
    return id == EMELA_I2C_ID_I2C1 ? OK : UNSUPPORTED;
}
static bool valid_timeout(int64_t timeout) { return timeout > 0 && (uint64_t)timeout <= UINT32_MAX; }
static int32_t context(EmelaI2cId id, bool require_configuration) {
    int32_t status = valid_id(id);
    if (status) return status;
    if (__get_IPSR() || __get_PRIMASK()) return INVALID_STATE;
    if (require_configuration && !configured) return NOT_CONFIGURED;
    return OK;
}
static int32_t acquire(void) {
    uint32_t previous = stm32f4_interrupt_lock();
    bool was_occupied = occupied;
    if (!was_occupied) occupied = true;
    stm32f4_interrupt_unlock(previous);
    return was_occupied ? BUSY : OK;
}
static int32_t error_status(uint32_t error) {
    if (error & HAL_I2C_ERROR_ARLO) return ARBITRATION_LOST;
    if (error & (HAL_I2C_ERROR_BERR | HAL_I2C_ERROR_OVR)) return BUS_ERROR;
    if (error & HAL_I2C_ERROR_AF) return NACK;
    if (error & HAL_I2C_ERROR_TIMEOUT) return TIMEOUT;
    return error ? OTHER : OK;
}
static void alternate_pins(void) {
    GPIO_InitTypeDef init = {.Mode = GPIO_MODE_AF_OD, .Pull = GPIO_NOPULL,
        .Speed = GPIO_SPEED_FREQ_VERY_HIGH, .Alternate = EMELA_I2C1_AF};
    for (unsigned i = 0; i < 2; ++i) {
        init.Pin = pin_bit(pins[i]);
        HAL_GPIO_Init(pin_port(pins[i]), &init);
    }
}
static void reset_controller(void) {
    uint32_t previous = stm32f4_interrupt_lock();
    __HAL_I2C_DISABLE_IT(&controller, I2C_IT_EVT | I2C_IT_BUF | I2C_IT_ERR);
    __HAL_RCC_I2C1_FORCE_RESET();
    __HAL_RCC_I2C1_RELEASE_RESET();
    controller.State = HAL_I2C_STATE_RESET;
    controller.Lock = HAL_UNLOCKED;
    stop_deferred = false;
    (void)HAL_I2C_Init(&controller); /* Previously validated configuration. */
    NVIC_ClearPendingIRQ(I2C1_EV_IRQn);
    NVIC_ClearPendingIRQ(I2C1_ER_IRQn);
    stm32f4_interrupt_unlock(previous);
}
static int32_t finish(int32_t status) {
    if (status) {
        uint32_t previous = stm32f4_interrupt_lock();
        /* Never send STOP after losing arbitration to another master. */
        if (status != ARBITRATION_LOST && (controller.Instance->SR2 & I2C_SR2_MSL))
            SET_BIT(controller.Instance->CR1, I2C_CR1_STOP);
        reset_controller();
        stm32f4_interrupt_unlock(previous);
    }
    occupied = false;
    return status;
}
static int32_t idle(uint64_t deadline) {
    while ((controller.Instance->SR2 & I2C_SR2_BUSY) || (controller.Instance->CR1 & I2C_CR1_STOP)) {
        if (expired(deadline)) return TIMEOUT;
    }
    return expired(deadline) ? TIMEOUT : OK;
}
static void poll_phase(void) {
    /* Pending bits are latched even while NVIC delivery is disabled. Reading
       SR1 here would make HAL's following SR2 read clear ADDR prematurely. */
    if (NVIC_GetPendingIRQ(I2C1_ER_IRQn)) {
        NVIC_ClearPendingIRQ(I2C1_ER_IRQn);
        HAL_I2C_ER_IRQHandler(&controller);
    }
    if (!phase_done && NVIC_GetPendingIRQ(I2C1_EV_IRQn)) {
        NVIC_ClearPendingIRQ(I2C1_EV_IRQn);
        stm32f4_i2c_event(&controller, &stop_deferred);
    }
}
static int32_t phase(uint16_t address, uint8_t *data, uint16_t count,
                     bool receive, bool last, uint64_t deadline) {
    if (expired(deadline)) return TIMEOUT;
    phase_done = false;
    phase_error = HAL_I2C_ERROR_NONE;
    stop_deferred = false;
    /* FIRST_AND_NEXT / LAST avoid HAL's independent fixed 25 ms BUSY wait.
       idle() already owns that deadline; PreviousState selects START/restart. */
    uint32_t option = last ? I2C_LAST_FRAME : I2C_FIRST_AND_NEXT_FRAME;
    HAL_StatusTypeDef status = receive
        ? HAL_I2C_Master_Seq_Receive_IT(&controller, address, data, count, option)
        : HAL_I2C_Master_Seq_Transmit_IT(&controller, address, data, count, option);
    if (status != HAL_OK) return status == HAL_BUSY ? BUSY : OTHER;
    while (!phase_done) {
        if (expired(deadline)) return TIMEOUT;
        if (stop_deferred) stm32f4_i2c_resume_event(&controller, &stop_deferred);
        if (!interrupts_enabled) poll_phase();
    }
    if (expired(deadline)) return TIMEOUT;
    int32_t result = error_status(phase_error);
    if (result) return result;
    if (controller.XferCount) return BUS_ERROR;
    return last ? idle(deadline) : OK;
}
static int32_t validate_words(const uint16_t *data, uint16_t length) {
    if (!data && length) return INVALID_ARGUMENT;
    for (uint32_t i = 0; i < length; ++i) if (data[i] > 255) return INVALID_ARGUMENT;
    return OK;
}
static int32_t send_words(uint16_t address, const uint16_t *data, uint16_t length,
                          bool last, uint64_t deadline) {
    uint32_t position = 0;
    do {
        uint32_t count = length - position;
        if (count > sizeof(bytes)) count = sizeof(bytes);
        for (uint32_t i = 0; i < count; ++i) bytes[i] = (uint8_t)data[position + i];
        position += count;
        int32_t status = phase(address, bytes, (uint16_t)count, false,
                               last && position == length, deadline);
        if (status) return status;
    } while (position < length);
    return OK;
}
static int32_t begin(EmelaI2cId id, int32_t address, int64_t timeout, uint64_t *deadline) {
    int32_t status = context(id, true);
    if (status) return status;
    if (address < 0 || address > 127 || !valid_timeout(timeout)) return INVALID_ARGUMENT;
    status = acquire();
    if (status) return status;
    *deadline = stm32f4_now_us() + (uint64_t)timeout * 1000;
    status = idle(*deadline);
    if (status) return finish(status);
    return OK;
}

int32_t emela_i2c_configure(EmelaI2cId id, EmelaI2cConfig config) {
    int32_t status = context(id, false);
    if (status) return status;
    if (config.clock_speed_hz <= 0 || config.duty_cycle < EMELA_I2C_DUTY_CYCLE_STANDARD
        || config.duty_cycle > EMELA_I2C_DUTY_CYCLE_FAST16_TO9) return INVALID_ARGUMENT;
    if (config.clock_speed_hz > 400000
        || (uint32_t)config.clock_speed_hz < (HAL_RCC_GetPCLK1Freq() + 8190 - 1) / 8190) return UNSUPPORTED;
    status = acquire();
    if (status) return status;
    int32_t claim = stm32f4_pins_claim(STM32F4_OWNER_I2C1, pins, 2);
    if (claim) { occupied = false; return claim == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT; }
    for (unsigned i = 0; i < 2; ++i) stm32f4_enable_gpio_clock((uint32_t)pins[i] / 16);
    __HAL_RCC_I2C1_CLK_ENABLE();
    alternate_pins();
    controller.Instance = I2C1;
    controller.Init = (I2C_InitTypeDef){.ClockSpeed = (uint32_t)config.clock_speed_hz,
        .DutyCycle = config.duty_cycle == EMELA_I2C_DUTY_CYCLE_STANDARD ? I2C_DUTYCYCLE_2 : I2C_DUTYCYCLE_16_9,
        .AddressingMode = I2C_ADDRESSINGMODE_7BIT, .DualAddressMode = I2C_DUALADDRESS_DISABLE,
        .GeneralCallMode = I2C_GENERALCALL_DISABLE, .NoStretchMode = I2C_NOSTRETCH_DISABLE};
    if (HAL_I2C_Init(&controller) != HAL_OK) {
        configured = false;
        occupied = false;
        stm32f4_pins_release(STM32F4_OWNER_I2C1, pins, 2);
        return OTHER;
    }
    configured = true;
    occupied = false;
    return OK;
}

int32_t emela_i2c_master_transmit(EmelaI2cId id, int32_t address, const uint16_t *data,
                                 uint16_t length, int64_t timeout) {
    int32_t status = validate_words(data, length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    return finish(send_words((uint16_t)(address << 1), data, length, true, deadline));
}

int32_t emela_i2c_write_read(EmelaI2cId id, int32_t address, const uint16_t *write_data,
                            uint16_t write_length, int32_t read_size, int64_t timeout,
                            uint16_t *out, uint16_t capacity, uint16_t *length) {
    if (!length || !out || read_size <= 0) return INVALID_ARGUMENT;
    *length = 0;
    if ((uint32_t)read_size > capacity || (uint32_t)read_size > sizeof(bytes)) return CAPACITY_EXCEEDED;
    int32_t status = validate_words(write_data, write_length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    uint16_t shifted = (uint16_t)(address << 1);
    if (write_length) status = send_words(shifted, write_data, write_length, false, deadline);
    if (!status) status = phase(shifted, bytes, (uint16_t)read_size, true, true, deadline);
    if (!status) {
        for (uint32_t i = 0; i < (uint32_t)read_size; ++i) out[i] = bytes[i];
        *length = (uint16_t)read_size;
    }
    return finish(status);
}

int32_t emela_i2c_memory_write(EmelaI2cId id, int32_t address, int32_t memory_address,
                              EmelaI2cMemoryAddressSize address_size, const uint16_t *data,
                              uint16_t length, int64_t timeout) {
    if (address_size < EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS8 || address_size > EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS16
        || memory_address < 0 || memory_address > (address_size == EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS8 ? 255 : 65535))
        return INVALID_ARGUMENT;
    int32_t status = validate_words(data, length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    uint16_t prefix[] = {(uint16_t)((uint32_t)memory_address >> 8), (uint16_t)(memory_address & 255)};
    bool wide = address_size == EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS16;
    status = send_words((uint16_t)(address << 1), wide ? prefix : prefix + 1, wide ? 2 : 1, !length, deadline);
    if (!status && length) status = send_words((uint16_t)(address << 1), data, length, true, deadline);
    return finish(status);
}

int32_t emela_i2c_is_device_ready(EmelaI2cId id, int32_t address, int32_t trials,
                                 int64_t timeout, int32_t *out) {
    if (!out || trials <= 0) return INVALID_ARGUMENT;
    *out = 0;
    uint64_t deadline;
    int32_t status = begin(id, address, timeout, &deadline);
    if (status) return status;
    for (int32_t attempt = 0; attempt < trials; ++attempt) {
        status = phase((uint16_t)(address << 1), bytes, 0, false, true, deadline);
        if (!status) { *out = 1; return finish(OK); }
        if (status != NACK) return finish(status);
        reset_controller();
        status = idle(deadline);
        if (status) return finish(status);
    }
    return finish(OK);
}

int32_t emela_i2c_enable_interrupt(EmelaI2cId id, EmelaInterruptPriority priority) {
    int32_t status = context(id, true);
    if (status) return status;
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH) return INVALID_ARGUMENT;
    status = acquire();
    if (status) return status;
    uint32_t previous = stm32f4_interrupt_lock();
    NVIC_SetPriority(I2C1_EV_IRQn, 12 - (uint32_t)priority * 4);
    NVIC_SetPriority(I2C1_ER_IRQn, 12 - (uint32_t)priority * 4);
    NVIC_ClearPendingIRQ(I2C1_EV_IRQn);
    NVIC_ClearPendingIRQ(I2C1_ER_IRQn);
    interrupts_enabled = true;
    NVIC_EnableIRQ(I2C1_EV_IRQn);
    NVIC_EnableIRQ(I2C1_ER_IRQn);
    stm32f4_interrupt_unlock(previous);
    occupied = false;
    return OK;
}
int32_t emela_i2c_disable_interrupt(EmelaI2cId id) {
    int32_t status = context(id, true);
    if (status) return status;
    status = acquire();
    if (status) return status;
    uint32_t previous = stm32f4_interrupt_lock();
    NVIC_DisableIRQ(I2C1_EV_IRQn);
    NVIC_DisableIRQ(I2C1_ER_IRQn);
    interrupts_enabled = false;
    stm32f4_interrupt_unlock(previous);
    occupied = false;
    return OK;
}

int32_t emela_i2c_recover(EmelaI2cId id, int64_t timeout) {
    int32_t status = context(id, true);
    if (status) return status;
    if (!valid_timeout(timeout)) return INVALID_ARGUMENT;
    status = acquire();
    if (status) return status;
    uint64_t deadline = stm32f4_now_us() + (uint64_t)timeout * 1000;
    __HAL_I2C_DISABLE_IT(&controller, I2C_IT_EVT | I2C_IT_BUF | I2C_IT_ERR);
    __HAL_I2C_DISABLE(&controller);
    GPIO_TypeDef *scl_port = pin_port(pins[0]), *sda_port = pin_port(pins[1]);
    uint16_t scl = pin_bit(pins[0]), sda = pin_bit(pins[1]);
    HAL_GPIO_WritePin(scl_port, scl, GPIO_PIN_SET);
    HAL_GPIO_WritePin(sda_port, sda, GPIO_PIN_SET);
    GPIO_InitTypeDef init = {.Mode = GPIO_MODE_OUTPUT_OD, .Pull = GPIO_NOPULL, .Speed = GPIO_SPEED_FREQ_LOW};
    for (unsigned i = 0; i < 2; ++i) { init.Pin = pin_bit(pins[i]); HAL_GPIO_Init(pin_port(pins[i]), &init); }
    for (unsigned pulse = 0; pulse < 9 && HAL_GPIO_ReadPin(sda_port, sda) == GPIO_PIN_RESET; ++pulse) {
        HAL_GPIO_WritePin(scl_port, scl, GPIO_PIN_RESET);
        if (!stm32f4_delay_us(5, deadline)) { status = TIMEOUT; break; }
        HAL_GPIO_WritePin(scl_port, scl, GPIO_PIN_SET);
        while (HAL_GPIO_ReadPin(scl_port, scl) == GPIO_PIN_RESET && !expired(deadline)) {}
        if (!stm32f4_delay_us(5, deadline)) { status = TIMEOUT; break; }
    }
    if (!status) {
        /* Generate a STOP even when SDA was already released. */
        HAL_GPIO_WritePin(scl_port, scl, GPIO_PIN_RESET);
        HAL_GPIO_WritePin(sda_port, sda, GPIO_PIN_RESET);
        if (!stm32f4_delay_us(5, deadline)) status = TIMEOUT;
        HAL_GPIO_WritePin(scl_port, scl, GPIO_PIN_SET);
        while (HAL_GPIO_ReadPin(scl_port, scl) == GPIO_PIN_RESET && !expired(deadline)) {}
        if (!stm32f4_delay_us(5, deadline)) status = TIMEOUT;
        HAL_GPIO_WritePin(sda_port, sda, GPIO_PIN_SET);
        if (!stm32f4_delay_us(5, deadline)) status = TIMEOUT;
        if (!status && (HAL_GPIO_ReadPin(scl_port, scl) == GPIO_PIN_RESET
            || HAL_GPIO_ReadPin(sda_port, sda) == GPIO_PIN_RESET)) status = BUS_ERROR;
    }
    /* Restore alternate-function pins and the saved configuration on every exit. */
    alternate_pins();
    reset_controller();
    if (!status) status = idle(deadline);
    occupied = false;
    return status;
}

void HAL_I2C_MasterTxCpltCallback(I2C_HandleTypeDef *handle) { if (handle == &controller) phase_done = true; }
void HAL_I2C_MasterRxCpltCallback(I2C_HandleTypeDef *handle) { if (handle == &controller) phase_done = true; }
void HAL_I2C_ErrorCallback(I2C_HandleTypeDef *handle) {
    if (handle == &controller) { phase_error = handle->ErrorCode; phase_done = true; }
}
void I2C1_EV_IRQHandler(void) { stm32f4_i2c_event(&controller, &stop_deferred); }
void I2C1_ER_IRQHandler(void) { HAL_I2C_ER_IRQHandler(&controller); }
