/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emela_i2c.h"
#include "hardware/clocks.h"
#include "hardware/gpio.h"
#include "hardware/i2c.h"
#include "hardware/irq.h"
#include "hardware/sync.h"

enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, NACK, BUS_ERROR,
       ARBITRATION_LOST, OTHER };
#define ERROR_INTERRUPTS (I2C_IC_INTR_MASK_M_TX_ABRT_BITS | I2C_IC_INTR_MASK_M_TX_OVER_BITS \
    | I2C_IC_INTR_MASK_M_RX_OVER_BITS | I2C_IC_INTR_MASK_M_RX_UNDER_BITS)
static const int32_t pins[] = {EMELA_I2C1_SCL_PIN, EMELA_I2C1_SDA_PIN};
static bool configured, occupied, irq_enabled;
static uint32_t frequency;
static struct {
    const uint16_t *first, *second;
    uint16_t first_length, second_length;
    uint32_t tx_index;
    uint16_t *received;
    uint16_t rx_length, rx_index;
    bool pending, reading;
    volatile int32_t error;
    volatile bool done;
} transfer;

static bool valid_timeout(int64_t timeout) { return timeout > 0 && (uint64_t)timeout <= UINT32_MAX; }
static int32_t valid_id(EmelaI2cId id) {
    if (id < EMELA_I2C_ID_I2C1 || id > EMELA_I2C_ID_I2C3) return INVALID_ARGUMENT;
    return id == EMELA_I2C_ID_I2C1 ? OK : UNSUPPORTED;
}
static int32_t enter(EmelaI2cId id, bool require_configured) {
    int32_t status = valid_id(id);
    if (status) return status;
    if (get_core_num() || emelino_pico_exception_number()) return INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (previous & 1u) status = INVALID_STATE;
    else if (require_configured && !configured) status = NOT_CONFIGURED;
    else if (occupied) status = BUSY;
    else occupied = true;
    emelino_pico_interrupt_unlock(previous);
    return status;
}
static int32_t leave(int32_t status) {
    uint32_t previous = emelino_pico_interrupt_lock();
    occupied = false;
    emelino_pico_interrupt_unlock(previous);
    return status;
}
static int32_t validate_words(const uint16_t *words, uint16_t length) {
    if (!words && length) return INVALID_ARGUMENT;
    for (uint16_t i = 0; i < length; ++i) if (words[i] > 255) return INVALID_ARGUMENT;
    return OK;
}
static int32_t abort_status(uint32_t reason) {
    if (reason & I2C_IC_TX_ABRT_SOURCE_ARB_LOST_BITS) return ARBITRATION_LOST;
    if (reason & (I2C_IC_TX_ABRT_SOURCE_ABRT_7B_ADDR_NOACK_BITS
        | I2C_IC_TX_ABRT_SOURCE_ABRT_TXDATA_NOACK_BITS | I2C_IC_TX_ABRT_SOURCE_ABRT_GCALL_NOACK_BITS)) return NACK;
    return BUS_ERROR;
}
static void bus_pins(void) {
    for (uint16_t i = 0; i < 2; ++i) {
        gpio_set_function(pins[i], GPIO_FUNC_I2C);
        gpio_pull_up(pins[i]);
    }
}
static void restore_controller(void) {
    /* A reset aborts a stuck transfer without a blocking SDK data operation. */
    i2c_init(i2c0, frequency);
    i2c_get_hw(i2c0)->intr_mask = 0;
    i2c_get_hw(i2c0)->dma_cr = 0;
    (void)i2c_get_hw(i2c0)->clr_intr;
    irq_clear(I2C0_IRQ);
}

/* Polling and interrupt mode use the same byte sequencer. At most one command
   is outstanding, preventing RX overflow and keeping write/ACK errors precise.
   No uint16_t public buffer is reinterpreted as packed bytes. */
static void progress(void) {
    i2c_hw_t *hw = i2c_get_hw(i2c0);
    if (transfer.done) { hw->intr_mask = 0; return; }
    uint32_t events = hw->raw_intr_stat;
    if (events & ERROR_INTERRUPTS) {
        transfer.error = events & I2C_IC_RAW_INTR_STAT_TX_ABRT_BITS
            ? abort_status(hw->tx_abrt_source) : BUS_ERROR;
        (void)hw->clr_intr;
        hw->intr_mask = 0;
        transfer.done = true;
        return;
    }
    if (transfer.pending) {
        if (transfer.reading && hw->rxflr) {
            transfer.received[transfer.rx_index++] = (uint16_t)(hw->data_cmd & 255u);
            transfer.pending = false;
        } else if (!transfer.reading && (events & I2C_IC_RAW_INTR_STAT_TX_EMPTY_BITS)) {
            transfer.pending = false;
        }
    }
    uint32_t total = (uint32_t)transfer.first_length + transfer.second_length;
    if (events & I2C_IC_RAW_INTR_STAT_STOP_DET_BITS) {
        (void)hw->clr_stop_det;
        if (transfer.pending || transfer.tx_index != total || transfer.rx_index != transfer.rx_length)
            transfer.error = BUS_ERROR;
        hw->intr_mask = 0;
        transfer.done = true;
        return;
    }
    if (!transfer.pending && (hw->status & I2C_IC_STATUS_TFNF_BITS)) {
        if (transfer.tx_index < total) {
            uint32_t index = transfer.tx_index++;
            uint32_t command = index < transfer.first_length ? transfer.first[index]
                : transfer.second[index - transfer.first_length];
            if (transfer.tx_index == total && !transfer.rx_length) command |= I2C_IC_DATA_CMD_STOP_BITS;
            transfer.reading = false;
            transfer.pending = true;
            hw->data_cmd = command;
        } else if (transfer.rx_index < transfer.rx_length) {
            uint32_t command = I2C_IC_DATA_CMD_CMD_BITS;
            if (total && !transfer.rx_index) command |= I2C_IC_DATA_CMD_RESTART_BITS;
            if (transfer.rx_index + 1 == transfer.rx_length) command |= I2C_IC_DATA_CMD_STOP_BITS;
            transfer.reading = true;
            transfer.pending = true;
            hw->data_cmd = command;
        }
    }
    if (irq_enabled) {
        uint32_t mask = ERROR_INTERRUPTS | I2C_IC_INTR_MASK_M_STOP_DET_BITS;
        if (transfer.pending) mask |= transfer.reading ? I2C_IC_INTR_MASK_M_RX_FULL_BITS : I2C_IC_INTR_MASK_M_TX_EMPTY_BITS;
        hw->intr_mask = mask;
    }
}
static void i2c_interrupt(void) { progress(); }

static int32_t transaction(int32_t address, const uint16_t *first, uint16_t first_length,
                           const uint16_t *second, uint16_t second_length,
                           uint16_t *received, uint16_t rx_length, uint64_t deadline) {
    /* This controller attaches START/STOP to data commands. There is no honest
       address-only transaction: do not read/write a dummy byte as a probe. */
    if (!first_length && !second_length && !rx_length) return UNSUPPORTED;
    if (emelino_pico_now_us() >= deadline) return TIMEOUT;
    i2c_hw_t *hw = i2c_get_hw(i2c0);
    hw->intr_mask = 0;
    hw->enable = 0;
    while (hw->enable_status & I2C_IC_ENABLE_STATUS_IC_EN_BITS) {
        if (emelino_pico_now_us() >= deadline) { restore_controller(); return TIMEOUT; }
    }
    uint32_t previous = emelino_pico_interrupt_lock();
    (void)hw->clr_intr;
    transfer.first = first;
    transfer.first_length = first_length;
    transfer.second = second;
    transfer.second_length = second_length;
    transfer.received = received;
    transfer.rx_length = rx_length;
    transfer.tx_index = transfer.rx_index = 0;
    transfer.error = OK;
    transfer.done = transfer.pending = transfer.reading = false;
    hw->tar = (uint32_t)address;
    hw->enable = 1;
    irq_clear(I2C0_IRQ);
    progress();
    emelino_pico_interrupt_unlock(previous);
    while (!transfer.done) {
        if (emelino_pico_now_us() >= deadline) {
            previous = emelino_pico_interrupt_lock();
            hw->intr_mask = 0;
            transfer.done = true;
            restore_controller();
            emelino_pico_interrupt_unlock(previous);
            return TIMEOUT;
        }
        if (!irq_enabled) progress();
    }
    int32_t status = transfer.error;
    if (emelino_pico_now_us() >= deadline) status = TIMEOUT;
    if (status) restore_controller();
    return status;
}
static int32_t begin(EmelaI2cId id, int32_t address, int64_t timeout, uint64_t *deadline) {
    if (address < 0 || address > 127 || !valid_timeout(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    *deadline = emelino_pico_now_us() + (uint64_t)timeout * 1000;
    return OK;
}

int32_t emela_i2c_configure(EmelaI2cId id, EmelaI2cConfig config) {
    if (config.clock_speed_hz <= 0 || config.duty_cycle < EMELA_I2C_DUTY_CYCLE_STANDARD
        || config.duty_cycle > EMELA_I2C_DUTY_CYCLE_FAST16_TO9) return INVALID_ARGUMENT;
    if (config.duty_cycle != EMELA_I2C_DUTY_CYCLE_STANDARD || config.clock_speed_hz > 1000000) return UNSUPPORTED;
    uint32_t clock = clock_get_hz(clk_sys), requested = (uint32_t)config.clock_speed_hz;
    uint32_t period = (clock + requested / 2) / requested;
    uint32_t low = period * 3 / 5, high = period - low;
    /* Check SDK divisor preconditions before claiming or reconfiguring pins. */
    if (!clock || low < 8 || high < 8 || low > 65535 || high > 65535) return UNSUPPORTED;
    int32_t status = enter(id, false);
    if (status) return status;
    int32_t claim = emelino_pico_pins_claim(EMELINO_PICO_OWNER_I2C0, pins, 2);
    if (claim) return leave(claim == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
    uint32_t previous = emelino_pico_interrupt_lock();
    frequency = requested;
    bus_pins();
    restore_controller();
    transfer.done = true;
    configured = true;
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_i2c_master_transmit(EmelaI2cId id, int32_t address, const uint16_t *data,
                                 uint16_t length, int64_t timeout) {
    int32_t status = validate_words(data, length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    return leave(transaction(address, data, length, 0, 0, 0, 0, deadline));
}
int32_t emela_i2c_write_read(EmelaI2cId id, int32_t address, const uint16_t *data,
                            uint16_t write_length, int32_t read_size, int64_t timeout,
                            uint16_t *out, uint16_t capacity, uint16_t *length) {
    if (length) *length = 0;
    if (!length || !out || read_size <= 0) return INVALID_ARGUMENT;
    if ((uint32_t)read_size > capacity) return CAPACITY_EXCEEDED;
    int32_t status = validate_words(data, write_length);
    if (status) return status;
    uint64_t deadline;
    status = begin(id, address, timeout, &deadline);
    if (status) return status;
    status = transaction(address, data, write_length, 0, 0, out, (uint16_t)read_size, deadline);
    if (!status) *length = (uint16_t)read_size;
    return leave(status);
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
    return leave(transaction(address, wide ? prefix : prefix + 1, wide ? 2 : 1,
                              data, length, 0, 0, deadline));
}
int32_t emela_i2c_is_device_ready(EmelaI2cId id, int32_t address, int32_t trials,
                                 int64_t timeout, int32_t *out) {
    if (!out || trials <= 0) return INVALID_ARGUMENT;
    *out = 0;
    uint64_t deadline;
    int32_t status = begin(id, address, timeout, &deadline);
    if (status) return status;
    return leave(UNSUPPORTED);
}
int32_t emela_i2c_enable_interrupt(EmelaI2cId id, EmelaInterruptPriority priority) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = emelino_pico_interrupt_lock();
    irq_enabled = true;
    irq_set_exclusive_handler(I2C0_IRQ, i2c_interrupt);
    irq_set_priority(I2C0_IRQ, 0xc0u - (uint32_t)priority * 0x40u);
    irq_clear(I2C0_IRQ);
    irq_set_enabled(I2C0_IRQ, true);
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_i2c_disable_interrupt(EmelaI2cId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = emelino_pico_interrupt_lock();
    irq_enabled = false;
    i2c_get_hw(i2c0)->intr_mask = 0;
    irq_set_enabled(I2C0_IRQ, false);
    irq_clear(I2C0_IRQ);
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
/* Open-drain recovery: output latch is always zero; input releases the wire. */
static void drive(int32_t pin, bool high) { gpio_set_dir(pin, high ? GPIO_IN : GPIO_OUT); }
static int32_t release_scl(uint64_t deadline) {
    drive(EMELA_I2C1_SCL_PIN, true);
    while (!gpio_get(EMELA_I2C1_SCL_PIN)) if (emelino_pico_now_us() >= deadline) return TIMEOUT;
    return emelino_pico_delay_us(5, deadline) ? OK : TIMEOUT;
}
int32_t emela_i2c_recover(EmelaI2cId id, int64_t timeout) {
    if (!valid_timeout(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    uint64_t deadline = emelino_pico_now_us() + (uint64_t)timeout * 1000;
    i2c_get_hw(i2c0)->intr_mask = 0;
    i2c_deinit(i2c0);
    for (uint16_t i = 0; i < 2; ++i) {
        gpio_set_dir(pins[i], GPIO_IN);
        gpio_put(pins[i], false);
        gpio_set_function(pins[i], GPIO_FUNC_SIO);
        gpio_pull_up(pins[i]);
    }
    status = release_scl(deadline);
    for (unsigned pulse = 0; !status && !gpio_get(EMELA_I2C1_SDA_PIN) && pulse < 9; ++pulse) {
        drive(EMELA_I2C1_SCL_PIN, false);
        if (!emelino_pico_delay_us(5, deadline)) status = TIMEOUT;
        if (!status) status = release_scl(deadline);
    }
    if (!status) {
        drive(EMELA_I2C1_SCL_PIN, false);
        drive(EMELA_I2C1_SDA_PIN, false);
        if (!emelino_pico_delay_us(5, deadline)) status = TIMEOUT;
        if (!status) status = release_scl(deadline);
        drive(EMELA_I2C1_SDA_PIN, true);
        if (!status && !emelino_pico_delay_us(5, deadline)) status = TIMEOUT;
        if (!status && !gpio_get(EMELA_I2C1_SDA_PIN)) status = BUS_ERROR;
    }
    /* Release both pins on every path, including a clock-stretch timeout. */
    drive(EMELA_I2C1_SCL_PIN, true);
    drive(EMELA_I2C1_SDA_PIN, true);
    bus_pins();
    restore_controller();
    if (!status && emelino_pico_now_us() >= deadline) status = TIMEOUT;
    return leave(status);
}
