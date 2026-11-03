/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emela_i2c.h"

/* I2c1 uses TWI0, leaving SPI2 independent. TX/RX task and byte-boundary
   sequencing follows the Nordic legacy TWI peripheral, in polling or IRQ mode. */
enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, NACK, BUS_ERROR,
       ARBITRATION_LOST, OTHER };
#define TWI_IRQ SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQn
#define TWI_INTERRUPTS (TWI_INTENSET_STOPPED_Msk | TWI_INTENSET_RXDREADY_Msk | \
                        TWI_INTENSET_TXDSENT_Msk | TWI_INTENSET_ERROR_Msk)
#define TWI_ERRORS (TWI_ERRORSRC_ANACK_Msk | TWI_ERRORSRC_DNACK_Msk | TWI_ERRORSRC_OVERRUN_Msk)
static const int32_t pins[] = {EMELA_I2C1_SCL_PIN, EMELA_I2C1_SDA_PIN};
static bool configured, occupied, irq_enabled;
static uint32_t frequency;
static struct {
    const uint16_t *first, *second;
    uint16_t first_length, second_length;
    uint32_t tx_index;
    uint16_t *received;
    uint16_t rx_length;
    volatile uint16_t rx_index;
    volatile int32_t error;
    volatile bool done;
    bool receiving;
} transfer;

static NRF_GPIO_Type *port(int32_t pin) { return pin < 32 ? NRF_P0 : NRF_P1; }
static uint32_t bit(int32_t pin) { return UINT32_C(1) << ((uint32_t)pin % 32); }
static bool pin_high(int32_t pin) { return (port(pin)->IN & bit(pin)) != 0; }
static void drive(int32_t pin, bool high) {
    if (high) port(pin)->OUTSET = bit(pin);
    else port(pin)->OUTCLR = bit(pin);
}
static void bus_pins(bool output) {
    for (uint16_t i = 0; i < 2; ++i) {
        drive(pins[i], true);
        port(pins[i])->PIN_CNF[pins[i] % 32] =
            (GPIO_PIN_CNF_PULL_Pullup << GPIO_PIN_CNF_PULL_Pos) |
            (GPIO_PIN_CNF_DRIVE_S0D1 << GPIO_PIN_CNF_DRIVE_Pos) |
            (output ? GPIO_PIN_CNF_DIR_Output << GPIO_PIN_CNF_DIR_Pos : 0);
    }
}
static int32_t valid_id(EmelaI2cId id) {
    if (id < EMELA_I2C_ID_I2C1 || id > EMELA_I2C_ID_I2C3) return INVALID_ARGUMENT;
    return id == EMELA_I2C_ID_I2C1 ? OK : UNSUPPORTED;
}
static bool valid_timeout(int64_t timeout) { return timeout > 0 && (uint64_t)timeout <= UINT32_MAX; }
static int32_t enter(EmelaI2cId id, bool require_configured) {
    int32_t status = valid_id(id);
    if (status) return status;
    if (nrf52_exception_number() || __get_PRIMASK()) return INVALID_STATE;
    uint32_t previous = nrf52_interrupt_lock();
    if (require_configured && !configured) status = NOT_CONFIGURED;
    else if (occupied) status = BUSY;
    else occupied = true;
    nrf52_interrupt_unlock(previous);
    return status;
}
static int32_t leave(int32_t status) {
    uint32_t previous = nrf52_interrupt_lock();
    occupied = false;
    nrf52_interrupt_unlock(previous);
    return status;
}
static int32_t error_status(uint32_t errors) {
    if (errors & TWI_ERRORSRC_OVERRUN_Msk) return BUS_ERROR;
    if (errors & (TWI_ERRORSRC_ANACK_Msk | TWI_ERRORSRC_DNACK_Msk)) return NACK;
    return OTHER;
}
static int32_t validate_words(const uint16_t *words, uint16_t length) {
    if (!words && length) return INVALID_ARGUMENT;
    for (uint16_t i = 0; i < length; ++i) if (words[i] > 255) return INVALID_ARGUMENT;
    return OK;
}
static void clear_events(void) {
    NRF_TWI0->EVENTS_STOPPED = 0;
    NRF_TWI0->EVENTS_TXDSENT = 0;
    NRF_TWI0->EVENTS_RXDREADY = 0;
    NRF_TWI0->EVENTS_ERROR = 0;
    NRF_TWI0->EVENTS_BB = 0;
    NRF_TWI0->EVENTS_SUSPENDED = 0;
    NRF_TWI0->ERRORSRC = TWI_ERRORS;
}
static void restore_controller(void) {
    NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
    NRF_TWI0->ENABLE = 0;
    NRF_TWI0->SHORTS = 0;
    clear_events();
    NRF_TWI0->PSEL.SCL = EMELA_I2C1_SCL_PIN;
    NRF_TWI0->PSEL.SDA = EMELA_I2C1_SDA_PIN;
    NRF_TWI0->FREQUENCY = frequency;
    NRF_TWI0->ENABLE = TWI_ENABLE_ENABLE_Enabled;
    NVIC_ClearPendingIRQ(TWI_IRQ);
}
static void receive_start(void) {
    transfer.receiving = true;
    NRF_TWI0->SHORTS = transfer.rx_length == 1 ? TWI_SHORTS_BB_STOP_Msk : TWI_SHORTS_BB_SUSPEND_Msk;
    /* Resume a previous TX suspension; STARTRX then generates the repeated
       START without ending the write transaction with a STOP. */
    NRF_TWI0->TASKS_RESUME = 1;
    NRF_TWI0->TASKS_STARTRX = 1;
}
static void send_next(void) {
    uint32_t index = transfer.tx_index++;
    NRF_TWI0->TXD = index < transfer.first_length ? transfer.first[index]
        : transfer.second[index - transfer.first_length];
}
static void progress(void) {
    if (NRF_TWI0->EVENTS_ERROR) {
        uint32_t errors = NRF_TWI0->ERRORSRC;
        NRF_TWI0->ERRORSRC = errors;
        NRF_TWI0->EVENTS_ERROR = 0;
        NRF_TWI0->EVENTS_TXDSENT = 0;
        NRF_TWI0->EVENTS_RXDREADY = 0;
        transfer.error = error_status(errors);
        NRF_TWI0->SHORTS = 0;
        NRF_TWI0->TASKS_RESUME = 1;
        NRF_TWI0->TASKS_STOP = 1;
    }
    if (!transfer.error && NRF_TWI0->EVENTS_TXDSENT) {
        NRF_TWI0->EVENTS_TXDSENT = 0;
        if (transfer.tx_index < (uint32_t)transfer.first_length + transfer.second_length) send_next();
        else if (transfer.rx_length) {
            NRF_TWI0->TASKS_SUSPEND = 1;
            receive_start();
        } else NRF_TWI0->TASKS_STOP = 1;
    }
    if (!transfer.error && transfer.receiving && NRF_TWI0->EVENTS_RXDREADY) {
        NRF_TWI0->EVENTS_RXDREADY = 0;
        uint16_t word = (uint16_t)NRF_TWI0->RXD;
        if (transfer.rx_index >= transfer.rx_length) {
            transfer.error = BUS_ERROR;
            NRF_TWI0->SHORTS = 0;
            NRF_TWI0->TASKS_RESUME = 1;
            NRF_TWI0->TASKS_STOP = 1;
        } else {
            transfer.received[transfer.rx_index++] = word;
            if (transfer.rx_index < transfer.rx_length) {
                NRF_TWI0->SHORTS = transfer.rx_index + 1 == transfer.rx_length
                    ? TWI_SHORTS_BB_STOP_Msk : TWI_SHORTS_BB_SUSPEND_Msk;
                NRF_TWI0->TASKS_RESUME = 1;
                /* RESUME may complete the final byte before this invocation
                   reaches STOPPED. Service its RXDREADY on the next pass first. */
                return;
            }
        }
    }
    /* RXDRDY and STOPPED can be pending together for the final byte. Read that
       byte before declaring completion, as in the vendor TWI driver. */
    if (NRF_TWI0->EVENTS_STOPPED) {
        NRF_TWI0->EVENTS_STOPPED = 0;
        NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
        if (!transfer.error && transfer.rx_index != transfer.rx_length) transfer.error = BUS_ERROR;
        transfer.done = true;
    }
}
void SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQHandler(void) {
    progress();
    (void)NRF_TWI0->INTENCLR; /* Synchronize the peripheral IRQ clear before exception return. */
    __DSB();
}

static int32_t transaction(int32_t address, const uint16_t *first, uint16_t first_length,
                           const uint16_t *second, uint16_t second_length,
                           uint16_t *received, uint16_t rx_length, uint64_t deadline) {
    if (nrf52_now_us() >= deadline) return TIMEOUT;
    uint32_t previous = nrf52_interrupt_lock();
    NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
    clear_events();
    NRF_TWI0->SHORTS = 0;
    transfer.first = first;
    transfer.first_length = first_length;
    transfer.second = second;
    transfer.second_length = second_length;
    transfer.received = received;
    transfer.rx_length = rx_length;
    transfer.tx_index = transfer.rx_index = 0;
    transfer.error = OK;
    transfer.done = transfer.receiving = false;
    NRF_TWI0->ADDRESS = (uint32_t)address;
    NVIC_ClearPendingIRQ(TWI_IRQ);
    if (irq_enabled) NRF_TWI0->INTENSET = TWI_INTERRUPTS;
    if (first_length || second_length || !rx_length) {
        NRF_TWI0->TASKS_RESUME = 1;
        NRF_TWI0->TASKS_STARTTX = 1;
        if (first_length || second_length) send_next();
        /* An empty TX is an address-only probe, matching nrfx_twi. Wait for
           STOPPED/ERROR, never infer ACK from a fixed delay or read a data byte. */
        else NRF_TWI0->TASKS_STOP = 1;
    } else receive_start();
    nrf52_interrupt_unlock(previous);
    while (!transfer.done) {
        if (nrf52_now_us() >= deadline) {
            previous = nrf52_interrupt_lock();
            NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
            NRF_TWI0->SHORTS = 0;
            NRF_TWI0->TASKS_RESUME = 1;
            NRF_TWI0->TASKS_STOP = 1;
            restore_controller();
            nrf52_interrupt_unlock(previous);
            return TIMEOUT;
        }
        if (!irq_enabled) progress();
    }
    int32_t status = transfer.error;
    if (nrf52_now_us() >= deadline) status = TIMEOUT;
    NRF_TWI0->SHORTS = 0;
    return status;
}
static int32_t begin(EmelaI2cId id, int32_t address, int64_t timeout, uint64_t *deadline) {
    if (address < 0 || address > 127 || !valid_timeout(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    *deadline = nrf52_now_us() + (uint64_t)timeout * 1000;
    return OK;
}

int32_t emela_i2c_configure(EmelaI2cId id, EmelaI2cConfig config) {
    if (config.clock_speed_hz <= 0 || config.duty_cycle < EMELA_I2C_DUTY_CYCLE_STANDARD
        || config.duty_cycle > EMELA_I2C_DUTY_CYCLE_FAST16_TO9) return INVALID_ARGUMENT;
    if (config.duty_cycle != EMELA_I2C_DUTY_CYCLE_STANDARD || config.clock_speed_hz < 100000) return UNSUPPORTED;
    uint32_t selected = config.clock_speed_hz >= 400000 ? TWI_FREQUENCY_FREQUENCY_K400
        : config.clock_speed_hz >= 250000 ? TWI_FREQUENCY_FREQUENCY_K250 : TWI_FREQUENCY_FREQUENCY_K100;
    int32_t status = enter(id, false);
    if (status) return status;
    int32_t claim = nrf52_pins_claim(NRF52_OWNER_TWI0, pins, 2);
    if (claim) return leave(claim == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
    uint32_t previous = nrf52_interrupt_lock();
    frequency = selected;
    bus_pins(false);
    restore_controller();
    configured = true;
    nrf52_interrupt_unlock(previous);
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
    for (int32_t i = 0; i < trials; ++i) {
        status = transaction(address, 0, 0, 0, 0, 0, 0, deadline);
        if (!status) { *out = 1; break; }
        if (status != NACK) return leave(status);
    }
    return leave(OK);
}
int32_t emela_i2c_enable_interrupt(EmelaI2cId id, EmelaInterruptPriority priority) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = nrf52_interrupt_lock();
    irq_enabled = true;
    NVIC_SetPriority(TWI_IRQ, 6 - (uint32_t)priority * 2);
    NVIC_ClearPendingIRQ(TWI_IRQ);
    NVIC_EnableIRQ(TWI_IRQ);
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_i2c_disable_interrupt(EmelaI2cId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = nrf52_interrupt_lock();
    irq_enabled = false;
    NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
    NVIC_DisableIRQ(TWI_IRQ);
    NVIC_ClearPendingIRQ(TWI_IRQ);
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}
static int32_t release_scl(uint64_t deadline) {
    drive(EMELA_I2C1_SCL_PIN, true);
    while (!pin_high(EMELA_I2C1_SCL_PIN)) if (nrf52_now_us() >= deadline) return TIMEOUT;
    return nrf52_delay_us(5, deadline) ? OK : TIMEOUT;
}
int32_t emela_i2c_recover(EmelaI2cId id, int64_t timeout) {
    if (!valid_timeout(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    uint64_t deadline = nrf52_now_us() + (uint64_t)timeout * 1000;
    NRF_TWI0->INTENCLR = TWI_INTERRUPTS;
    NRF_TWI0->ENABLE = 0;
    NRF_TWI0->PSEL.SCL = NRF_TWI0->PSEL.SDA = UINT32_MAX;
    bus_pins(true);
    status = release_scl(deadline);
    for (unsigned pulse = 0; !status && !pin_high(EMELA_I2C1_SDA_PIN) && pulse < 9; ++pulse) {
        drive(EMELA_I2C1_SCL_PIN, false);
        if (!nrf52_delay_us(5, deadline)) status = TIMEOUT;
        if (!status) status = release_scl(deadline);
    }
    if (!status) {
        /* STOP after the possible nine clocks: SDA rises while SCL is high. */
        drive(EMELA_I2C1_SCL_PIN, false);
        drive(EMELA_I2C1_SDA_PIN, false);
        if (!nrf52_delay_us(5, deadline)) status = TIMEOUT;
        if (!status) status = release_scl(deadline);
        drive(EMELA_I2C1_SDA_PIN, true);
        if (!status && !nrf52_delay_us(5, deadline)) status = TIMEOUT;
        if (!status && !pin_high(EMELA_I2C1_SDA_PIN)) status = BUS_ERROR;
    }
    /* Release both wires and restore the previous configuration on every path.
       A failed recovery makes no claim that the external bus is usable. */
    bus_pins(false);
    restore_controller();
    if (!status && nrf52_now_us() >= deadline) status = TIMEOUT;
    return leave(status);
}
