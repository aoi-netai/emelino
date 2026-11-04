/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emelino_spi.h"
#include "gpio_support.h"

/* SPI2 is independent of the SPI0/TWI0 and SPI1/TWI1 shared register blocks.
   The public Spi1 name denotes the first bus in this board's mapping. */
static bool pins_claimed;
static bool occupied;
/* At most one byte is in flight; READY can be lost independently of RXD. */
static bool rx_pending;
static const int32_t bus_pins[] = {
    EMELA_SPI1_SCK_PIN, EMELA_SPI1_MISO_PIN, EMELA_SPI1_MOSI_PIN,
};

static int32_t valid_config(EmelaSpiConfig config, uint32_t *frequency) {
    if (config.mode < EMELA_SPI_MODE_MASTER || config.mode > EMELA_SPI_MODE_SLAVE
        || config.direction < EMELA_SPI_DIRECTION_TWO_LINE || config.direction > EMELA_SPI_DIRECTION_ONE_LINE
        || config.data_size < EMELA_SPI_DATA_SIZE_BITS8 || config.data_size > EMELA_SPI_DATA_SIZE_BITS16
        || config.polarity < EMELA_SPI_CLOCK_POLARITY_IDLE_LOW || config.polarity > EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH
        || config.phase < EMELA_SPI_CLOCK_PHASE_FIRST_EDGE || config.phase > EMELA_SPI_CLOCK_PHASE_SECOND_EDGE
        || config.bit_order < EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST || config.bit_order > EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST
        || config.max_frequency_hz <= 0) return SPI_INVALID_ARGUMENT;
    if (config.mode != EMELA_SPI_MODE_MASTER || config.direction == EMELA_SPI_DIRECTION_ONE_LINE
        || config.data_size != EMELA_SPI_DATA_SIZE_BITS8) return SPI_UNSUPPORTED;
    static const struct { uint32_t hz, value; } rates[] = {
        {8000000, SPI_FREQUENCY_FREQUENCY_M8}, {4000000, SPI_FREQUENCY_FREQUENCY_M4},
        {2000000, SPI_FREQUENCY_FREQUENCY_M2}, {1000000, SPI_FREQUENCY_FREQUENCY_M1},
        {500000, SPI_FREQUENCY_FREQUENCY_K500}, {250000, SPI_FREQUENCY_FREQUENCY_K250},
        {125000, SPI_FREQUENCY_FREQUENCY_K125},
    };
    for (uint32_t i = 0; i < sizeof(rates) / sizeof(rates[0]); ++i) {
        if (rates[i].hz <= (uint32_t)config.max_frequency_hz) {
            *frequency = rates[i].value;
            return SPI_OK;
        }
    }
    return SPI_UNSUPPORTED;
}

static void configure_pin(int32_t pin, bool output, bool high) {
    NRF_GPIO_Type *port = nrf52_gpio_port((uint32_t)pin / 32);
    uint32_t bit = UINT32_C(1) << ((uint32_t)pin % 32);
    if (output) {
        if (high) port->OUTSET = bit;
        else port->OUTCLR = bit;
    }
    port->PIN_CNF[(uint32_t)pin % 32] =
        ((output ? GPIO_PIN_CNF_DIR_Output : GPIO_PIN_CNF_DIR_Input) << GPIO_PIN_CNF_DIR_Pos)
        /* The SCK input buffer must stay connected even in output mode. */
        | (GPIO_PIN_CNF_INPUT_Connect << GPIO_PIN_CNF_INPUT_Pos)
        | (GPIO_PIN_CNF_DRIVE_S0S1 << GPIO_PIN_CNF_DRIVE_Pos)
        | (GPIO_PIN_CNF_PULL_Disabled << GPIO_PIN_CNF_PULL_Pos);
}

static int32_t apply_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    uint32_t frequency;
    int32_t status = valid_config(config, &frequency);
    if (status) return status;
    if (!pins_claimed) {
        int32_t pin_status = nrf52_pins_claim(NRF52_OWNER_SPI2, bus_pins, 3);
        if (pin_status) return pin_status == 5 ? SPI_RESOURCE_CONFLICT : SPI_INVALID_ARGUMENT;
        pins_claimed = true;
    }
    NRF_SPI2->ENABLE = SPI_ENABLE_ENABLE_Disabled;
    NRF_SPI2->INTENCLR = SPI_INTENCLR_READY_Msk;
    configure_pin(EMELA_SPI1_SCK_PIN, true, config.polarity == EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH);
    bool receive_only = config.direction == EMELA_SPI_DIRECTION_TWO_LINE_RECEIVE_ONLY;
    configure_pin(EMELA_SPI1_MOSI_PIN, !receive_only, false);
    configure_pin(EMELA_SPI1_MISO_PIN, false, false);
    NRF_SPI2->PSEL.SCK = EMELA_SPI1_SCK_PIN;
    NRF_SPI2->PSEL.MOSI = receive_only ? UINT32_MAX : EMELA_SPI1_MOSI_PIN;
    NRF_SPI2->PSEL.MISO = EMELA_SPI1_MISO_PIN;
    NRF_SPI2->FREQUENCY = frequency;
    NRF_SPI2->CONFIG = ((uint32_t)config.bit_order << SPI_CONFIG_ORDER_Pos)
        | ((uint32_t)config.phase << SPI_CONFIG_CPHA_Pos)
        | ((uint32_t)config.polarity << SPI_CONFIG_CPOL_Pos);
    NRF_SPI2->EVENTS_READY = 0;
    NRF_SPI2->ENABLE = SPI_ENABLE_ENABLE_Enabled;
    return SPI_OK;
}

static int32_t validate_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    uint32_t frequency;
    return valid_config(config, &frequency);
}

static int32_t exchange(EmelinoSpiBus *bus, int32_t operation, const uint16_t *data,
                        uint16_t length, uint16_t *output, uint64_t deadline) {
    for (uint16_t i = 0; i < length; ++i) {
        if (nrf52_now_us() >= deadline) return SPI_TIMEOUT;
        NRF_SPI2->EVENTS_READY = 0;
        rx_pending = true;
        NRF_SPI2->TXD = operation == EMELA_SPI_OPERATION_READ ? UINT8_MAX : data[i];
        while (!NRF_SPI2->EVENTS_READY) {
            if (nrf52_now_us() >= deadline) {
                bus->recovery_required = true;
                NRF_SPI2->ENABLE = SPI_ENABLE_ENABLE_Disabled;
                return SPI_TIMEOUT;
            }
        }
        uint16_t value = (uint16_t)NRF_SPI2->RXD;
        rx_pending = false;
        NRF_SPI2->EVENTS_READY = 0;
        if (nrf52_now_us() >= deadline) {
            bus->recovery_required = true;
            return SPI_TIMEOUT;
        }
        if (output) output[i] = value;
    }
    return SPI_OK;
}

static int32_t recover(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    if (!pins_claimed) return SPI_NOT_CONFIGURED;
    NRF_SPI2->ENABLE = SPI_ENABLE_ENABLE_Disabled;
    /* Disabling SPI does not discard its buffered receive byte. Drain an
       unfinished exchange even if its READY event was lost or cleared. */
    if (rx_pending || NRF_SPI2->EVENTS_READY) (void)NRF_SPI2->RXD;
    rx_pending = false;
    NRF_SPI2->EVENTS_READY = 0;
    return apply_config(bus, config);
}

static int32_t cs_configure(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return nrf52_gpio_output_owned((uint16_t)(NRF52_OWNER_SPI_CS_BASE + slot), pin, level);
}
static int32_t cs_write(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return nrf52_gpio_write_owned((uint16_t)(NRF52_OWNER_SPI_CS_BASE + slot), pin, level);
}
static void cs_release(EmelinoSpiBus *bus, uint16_t slot, int32_t pin) {
    (void)bus;
    nrf52_gpio_release_owned((uint16_t)(NRF52_OWNER_SPI_CS_BASE + slot), pin);
}
static uint64_t now_us(EmelinoSpiBus *bus) { (void)bus; return nrf52_now_us(); }
static bool delay_us(EmelinoSpiBus *bus, uint64_t delay, uint64_t deadline) {
    (void)bus;
    return nrf52_delay_us(delay, deadline);
}
static const EmelinoSpiOps operations = {
    .validate_config = validate_config, .configure = apply_config, .exchange = exchange,
    .recover = recover, .cs_configure = cs_configure, .cs_write = cs_write,
    .cs_release = cs_release, .now_us = now_us, .delay_us = delay_us,
};
static EmelinoSpiBus bus1 = {.ops = &operations};

int32_t emelino_spi_lock(void) {
    if (nrf52_exception_number() || __get_PRIMASK()) return SPI_INVALID_STATE;
    uint32_t previous = nrf52_interrupt_lock();
    bool was_occupied = occupied;
    if (!was_occupied) occupied = true;
    nrf52_interrupt_unlock(previous);
    return was_occupied ? SPI_BUSY : SPI_OK;
}
void emelino_spi_unlock(void) {
    uint32_t previous = nrf52_interrupt_lock();
    occupied = false;
    nrf52_interrupt_unlock(previous);
}
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus) {
    if (id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6) return SPI_INVALID_ARGUMENT;
    if (id != EMELA_SPI_ID_SPI1) return SPI_UNSUPPORTED;
    *bus = &bus1;
    return SPI_OK;
}
