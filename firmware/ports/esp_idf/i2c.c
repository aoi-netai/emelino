/* SPDX-License-Identifier: Apache-2.0 */
#include <stdlib.h>
#include "board_config.h"
#include "bsp.h"
#include "emela_i2c.h"
#include "driver/i2c_master.h"

enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, NACK, BUS_ERROR, ARBITRATION_LOST, OTHER };
static const int32_t pins[] = {EMELA_I2C1_SCL_PIN, EMELA_I2C1_SDA_PIN};
static bool occupied;
static i2c_master_bus_handle_t bus;
static uint32_t frequency;

static int32_t enter(EmelaI2cId id, bool require_configured) {
    if (id < EMELA_I2C_ID_I2C1 || id > EMELA_I2C_ID_I2C3) return INVALID_ARGUMENT;
    if (id != EMELA_I2C_ID_I2C1) return UNSUPPORTED;
    if (!emelino_idf_task_context()) return INVALID_STATE;
    uint32_t previous = emelino_idf_lock();
    int32_t status = require_configured && !bus ? NOT_CONFIGURED : occupied ? BUSY : OK;
    if (!status) occupied = true;
    emelino_idf_unlock(previous);
    return status;
}
static int32_t leave(int32_t status) {
    uint32_t previous = emelino_idf_lock(); occupied = false; emelino_idf_unlock(previous);
    return status;
}
static int32_t from_error(esp_err_t error) {
    if (error == ESP_OK) return OK;
    if (error == ESP_ERR_TIMEOUT) return TIMEOUT;
    if (error == ESP_ERR_INVALID_RESPONSE || error == ESP_ERR_NOT_FOUND) return NACK;
    if (error == ESP_ERR_NO_MEM) return CAPACITY_EXCEEDED;
    if (error == ESP_ERR_INVALID_ARG) return INVALID_ARGUMENT;
    if (error == ESP_ERR_INVALID_STATE) return INVALID_STATE;
    return BUS_ERROR;
}
static int32_t words_valid(const uint16_t *data, uint16_t length) {
    if (!data && length) return INVALID_ARGUMENT;
    for (uint16_t i = 0; i < length; ++i) if (data[i] > 255) return INVALID_ARGUMENT;
    return OK;
}
static int32_t begin(EmelaI2cId id, int32_t address, int64_t timeout, uint64_t *deadline) {
    if (address < 0 || address > 127 || !emelino_idf_timeout_valid(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (!status) *deadline = emelino_idf_now_us() + (uint64_t)timeout * 1000;
    return status;
}
int32_t emela_i2c_configure(EmelaI2cId id, EmelaI2cConfig config) {
    if (config.clock_speed_hz <= 0 || config.duty_cycle < EMELA_I2C_DUTY_CYCLE_STANDARD
        || config.duty_cycle > EMELA_I2C_DUTY_CYCLE_FAST16_TO9) return INVALID_ARGUMENT;
    /* IDF chooses timing from Hz; STM32-specific duty ratios have no mapping. */
    if (config.clock_speed_hz > 400000 || config.clock_speed_hz < 1000
        || config.duty_cycle != EMELA_I2C_DUTY_CYCLE_STANDARD) return UNSUPPORTED;
    int32_t status = enter(id, false);
    if (status) return status;
    if (!bus) {
        status = emelino_idf_pins_claim(EMELINO_IDF_OWNER_I2C0, pins, 2);
        if (status) return leave(status == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
        i2c_master_bus_config_t native = {.i2c_port = I2C_NUM_0, .scl_io_num = EMELA_I2C1_SCL_PIN,
            .sda_io_num = EMELA_I2C1_SDA_PIN, .clk_source = I2C_CLK_SRC_DEFAULT,
            .glitch_ignore_cnt = 7, .flags.enable_internal_pullup = true};
        esp_err_t error = i2c_new_master_bus(&native, &bus);
        if (error != ESP_OK) {
            emelino_idf_pins_release(EMELINO_IDF_OWNER_I2C0, pins, 2);
            return leave(error == ESP_ERR_NOT_FOUND || error == ESP_ERR_INVALID_STATE
                ? RESOURCE_CONFLICT : from_error(error));
        }
    }
    frequency = (uint32_t)config.clock_speed_hz;
    return leave(OK);
}
static int32_t transaction(int32_t address, const uint16_t *first, uint16_t first_length,
                           const uint16_t *second, uint16_t second_length,
                           uint16_t *received, uint16_t rx_length, uint64_t deadline) {
    size_t tx_length = (size_t)first_length + second_length;
    int remaining = emelino_idf_remaining_ms(deadline);
    if (!remaining) return TIMEOUT;
    if (!tx_length && !rx_length) {
        esp_err_t error = i2c_master_probe(bus, address, remaining);
        return emelino_idf_now_us() >= deadline ? TIMEOUT : from_error(error);
    }
    uint8_t *tx = tx_length ? malloc(tx_length) : NULL;
    uint8_t *rx = rx_length ? malloc(rx_length) : NULL;
    if ((tx_length && !tx) || (rx_length && !rx)) { free(tx); free(rx); return CAPACITY_EXCEEDED; }
    for (size_t i = 0; i < tx_length; ++i) tx[i] = (uint8_t)(i < first_length ? first[i] : second[i - first_length]);
    i2c_device_config_t native = {.dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = (uint16_t)address, .scl_speed_hz = frequency};
    i2c_master_dev_handle_t device = NULL;
    esp_err_t error = i2c_master_bus_add_device(bus, &native, &device);
    if (error == ESP_OK) {
        remaining = emelino_idf_remaining_ms(deadline);
        if (!remaining) error = ESP_ERR_TIMEOUT;
        else if (tx_length && rx_length) error = i2c_master_transmit_receive(device, tx, tx_length, rx, rx_length, remaining);
        else if (rx_length) error = i2c_master_receive(device, rx, rx_length, remaining);
        else error = i2c_master_transmit(device, tx, tx_length, remaining);
        esp_err_t cleanup = i2c_master_bus_rm_device(device);
        if (error == ESP_OK) error = cleanup;
    }
    if (emelino_idf_now_us() >= deadline) error = ESP_ERR_TIMEOUT;
    if (error == ESP_OK) for (uint16_t i = 0; i < rx_length; ++i) received[i] = rx[i];
    free(tx); free(rx);
    return from_error(error);
}
int32_t emela_i2c_master_transmit(EmelaI2cId id, int32_t address, const uint16_t *data,
                                 uint16_t length, int64_t timeout) {
    int32_t status = words_valid(data, length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    return leave(transaction(address, data, length, NULL, 0, NULL, 0, deadline));
}
int32_t emela_i2c_write_read(EmelaI2cId id, int32_t address, const uint16_t *data,
                            uint16_t write_length, int32_t read_size, int64_t timeout,
                            uint16_t *out, uint16_t capacity, uint16_t *length) {
    if (length) *length = 0;
    if (!out || !length || read_size <= 0) return INVALID_ARGUMENT;
    if ((uint32_t)read_size > capacity) return CAPACITY_EXCEEDED;
    int32_t status = words_valid(data, write_length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    status = transaction(address, data, write_length, NULL, 0, out, (uint16_t)read_size, deadline);
    if (!status) *length = (uint16_t)read_size;
    return leave(status);
}
int32_t emela_i2c_memory_write(EmelaI2cId id, int32_t address, int32_t memory_address,
                              EmelaI2cMemoryAddressSize address_size, const uint16_t *data,
                              uint16_t length, int64_t timeout) {
    if (address_size < EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS8 || address_size > EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS16
        || memory_address < 0 || memory_address > (address_size == EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS8 ? 255 : 65535))
        return INVALID_ARGUMENT;
    int32_t status = words_valid(data, length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    uint16_t prefix[] = {(uint16_t)((uint32_t)memory_address >> 8), (uint16_t)(memory_address & 255)};
    bool wide = address_size == EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS16;
    return leave(transaction(address, wide ? prefix : prefix + 1, wide ? 2 : 1, data, length, NULL, 0, deadline));
}
int32_t emela_i2c_is_device_ready(EmelaI2cId id, int32_t address, int32_t trials, int64_t timeout, int32_t *out) {
    if (!out || trials <= 0) return INVALID_ARGUMENT;
    *out = 0;
    uint64_t deadline;
    int32_t status = begin(id, address, timeout, &deadline);
    if (status) return status;
    for (int32_t i = 0; i < trials; ++i) {
        status = transaction(address, NULL, 0, NULL, 0, NULL, 0, deadline);
        if (status == OK) { *out = 1; break; }
        if (status != NACK) break;
    }
    return leave(status == NACK ? OK : status);
}
int32_t emela_i2c_enable_interrupt(EmelaI2cId id, EmelaInterruptPriority priority) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    /* The synchronous IDF master driver owns its completion ISR. A public
       selectable polling/IRQ backend is not available through this API. */
    return leave(UNSUPPORTED);
}
int32_t emela_i2c_disable_interrupt(EmelaI2cId id) {
    int32_t status = enter(id, true);
    return status ? status : leave(OK);
}
int32_t emela_i2c_recover(EmelaI2cId id, int64_t timeout) {
    if (!emelino_idf_timeout_valid(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    uint64_t deadline = emelino_idf_now_us() + (uint64_t)timeout * 1000;
    /* IDF releases SDA, pulses SCL and restores the controller's timing. */
    status = from_error(i2c_master_bus_reset(bus));
    if (emelino_idf_now_us() >= deadline) status = TIMEOUT;
    return leave(status);
}
