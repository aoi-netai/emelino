/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emelino_spi.h"
#include "gpio_support.h"
#include "hardware/clocks.h"
#include "hardware/gpio.h"
#include "hardware/spi.h"
#include "hardware/sync.h"

/* The public first bus, Spi1, is the board's hardware SPI0. Device ownership,
   composite operations and all CS cleanup remain in firmware/common/spi.c. */
static bool pins_claimed, occupied;
static const int32_t bus_pins[] = {
    EMELA_SPI1_SCK_PIN, EMELA_SPI1_MISO_PIN, EMELA_SPI1_MOSI_PIN,
};

static int32_t valid_config(EmelaSpiConfig config, uint32_t *prescale, uint32_t *postdiv) {
    if (config.mode < EMELA_SPI_MODE_MASTER || config.mode > EMELA_SPI_MODE_SLAVE
        || config.direction < EMELA_SPI_DIRECTION_TWO_LINE || config.direction > EMELA_SPI_DIRECTION_ONE_LINE
        || config.data_size < EMELA_SPI_DATA_SIZE_BITS8 || config.data_size > EMELA_SPI_DATA_SIZE_BITS16
        || config.polarity < EMELA_SPI_CLOCK_POLARITY_IDLE_LOW || config.polarity > EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH
        || config.phase < EMELA_SPI_CLOCK_PHASE_FIRST_EDGE || config.phase > EMELA_SPI_CLOCK_PHASE_SECOND_EDGE
        || config.bit_order < EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST || config.bit_order > EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST
        || config.max_frequency_hz <= 0) return SPI_INVALID_ARGUMENT;
    if (config.mode != EMELA_SPI_MODE_MASTER || config.direction == EMELA_SPI_DIRECTION_ONE_LINE
        || config.bit_order != EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST) return SPI_UNSUPPORTED;
    uint32_t clock = clock_get_hz(clk_peri), best = UINT32_MAX;
    if (!clock) return SPI_UNSUPPORTED;
    /* Compare the rational frequency, not a rounded integer: even a fractional
       Hz above the requested maximum must not be accepted. */
    for (uint32_t pre = 2; pre <= 254; pre += 2) {
        uint64_t denominator = (uint64_t)pre * (uint32_t)config.max_frequency_hz;
        uint32_t post = (uint32_t)(((uint64_t)clock + denominator - 1) / denominator);
        if (post < 1) post = 1;
        if (post <= 256 && pre * post < best) {
            best = pre * post;
            *prescale = pre;
            *postdiv = post;
        }
    }
    return best == UINT32_MAX ? SPI_UNSUPPORTED : SPI_OK;
}

static int32_t apply_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    uint32_t prescale = 0, postdiv = 0;
    int32_t status = valid_config(config, &prescale, &postdiv);
    if (status) return status;
    if (!pins_claimed) {
        int32_t claim = emelino_pico_pins_claim(EMELINO_PICO_OWNER_SPI0, bus_pins, 3);
        if (claim) return claim == 5 ? SPI_RESOURCE_CONFLICT : SPI_INVALID_ARGUMENT;
        pins_claimed = true;
    }
    /* Reset also clears any stale FIFO contents after a failed transfer. */
    spi_init(spi0, clock_get_hz(clk_peri) / 2);
    spi_get_hw(spi0)->cr1 = 0;
    spi_get_hw(spi0)->imsc = 0;
    spi_get_hw(spi0)->dmacr = 0;
    spi_get_hw(spi0)->cpsr = prescale;
    spi_get_hw(spi0)->cr0 = ((postdiv - 1) << SPI_SSPCR0_SCR_LSB)
        | (config.data_size == EMELA_SPI_DATA_SIZE_BITS16 ? 15u : 7u)
        | (config.polarity == EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH ? SPI_SSPCR0_SPO_BITS : 0)
        | (config.phase == EMELA_SPI_CLOCK_PHASE_SECOND_EDGE ? SPI_SSPCR0_SPH_BITS : 0);
    gpio_set_function(EMELA_SPI1_SCK_PIN, GPIO_FUNC_SPI);
    gpio_set_function(EMELA_SPI1_MISO_PIN, GPIO_FUNC_SPI);
    gpio_disable_pulls(EMELA_SPI1_SCK_PIN);
    gpio_disable_pulls(EMELA_SPI1_MISO_PIN);
    gpio_disable_pulls(EMELA_SPI1_MOSI_PIN);
    if (config.direction == EMELA_SPI_DIRECTION_TWO_LINE_RECEIVE_ONLY) {
        gpio_set_function(EMELA_SPI1_MOSI_PIN, GPIO_FUNC_SIO);
        gpio_set_dir(EMELA_SPI1_MOSI_PIN, GPIO_IN);
    } else gpio_set_function(EMELA_SPI1_MOSI_PIN, GPIO_FUNC_SPI);
    spi_get_hw(spi0)->icr = SPI_SSPICR_RORIC_BITS | SPI_SSPICR_RTIC_BITS;
    spi_get_hw(spi0)->cr1 = SPI_SSPCR1_SSE_BITS;
    return SPI_OK;
}

static int32_t validate_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    uint32_t prescale, postdiv;
    return valid_config(config, &prescale, &postdiv);
}
static int32_t failed_exchange(EmelinoSpiBus *bus, int32_t status) {
    spi_get_hw(spi0)->cr1 = 0;
    bus->recovery_required = true;
    return status;
}
static int32_t exchange(EmelinoSpiBus *bus, int32_t operation, const uint16_t *data,
                        uint16_t length, uint16_t *output, uint64_t deadline) {
    uint16_t filler = bus->current_config.data_size == EMELA_SPI_DATA_SIZE_BITS16 ? UINT16_MAX : UINT8_MAX;
    /* One word in flight bounds FIFO use even if another IRQ delays the caller. */
    for (uint16_t i = 0; i < length; ++i) {
        while (!spi_is_writable(spi0)) {
            if (emelino_pico_now_us() >= deadline) return failed_exchange(bus, SPI_TIMEOUT);
        }
        if (emelino_pico_now_us() >= deadline) return failed_exchange(bus, SPI_TIMEOUT);
        spi_get_hw(spi0)->dr = operation == EMELA_SPI_OPERATION_READ ? filler : data[i];
        while (!spi_is_readable(spi0)) {
            if (emelino_pico_now_us() >= deadline) return failed_exchange(bus, SPI_TIMEOUT);
        }
        uint16_t value = (uint16_t)spi_get_hw(spi0)->dr;
        if (output) output[i] = value;
        if (spi_get_hw(spi0)->ris & SPI_SSPRIS_RORRIS_BITS) return failed_exchange(bus, SPI_OVERRUN);
    }
    /* RX completion may precede the final clock edge. CS must remain asserted. */
    while (spi_get_hw(spi0)->sr & SPI_SSPSR_BSY_BITS) {
        if (emelino_pico_now_us() >= deadline) return failed_exchange(bus, SPI_TIMEOUT);
    }
    return emelino_pico_now_us() >= deadline ? failed_exchange(bus, SPI_TIMEOUT) : SPI_OK;
}
static int32_t recover(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    return pins_claimed ? apply_config(bus, config) : SPI_NOT_CONFIGURED;
}
static int32_t cs_configure(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return emelino_pico_gpio_output_owned((uint16_t)(EMELINO_PICO_OWNER_SPI_CS_BASE + slot), pin, level);
}
static int32_t cs_write(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return emelino_pico_gpio_write_owned((uint16_t)(EMELINO_PICO_OWNER_SPI_CS_BASE + slot), pin, level);
}
static void cs_release(EmelinoSpiBus *bus, uint16_t slot, int32_t pin) {
    (void)bus;
    emelino_pico_gpio_release_owned((uint16_t)(EMELINO_PICO_OWNER_SPI_CS_BASE + slot), pin);
}
static uint64_t now_us(EmelinoSpiBus *bus) { (void)bus; return emelino_pico_now_us(); }
static bool delay_us(EmelinoSpiBus *bus, uint64_t delay, uint64_t deadline) {
    (void)bus;
    return emelino_pico_delay_us(delay, deadline);
}
static const EmelinoSpiOps operations = {
    .validate_config = validate_config, .configure = apply_config, .exchange = exchange,
    .recover = recover, .cs_configure = cs_configure, .cs_write = cs_write,
    .cs_release = cs_release, .now_us = now_us, .delay_us = delay_us,
};
static EmelinoSpiBus bus1 = {.ops = &operations};

int32_t emelino_spi_lock(void) {
    if (get_core_num() || emelino_pico_exception_number()) return SPI_INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (previous & 1u) {
        emelino_pico_interrupt_unlock(previous);
        return SPI_INVALID_STATE;
    }
    bool was_occupied = occupied;
    if (!was_occupied) occupied = true;
    emelino_pico_interrupt_unlock(previous);
    return was_occupied ? SPI_BUSY : SPI_OK;
}
void emelino_spi_unlock(void) {
    uint32_t previous = emelino_pico_interrupt_lock();
    occupied = false;
    emelino_pico_interrupt_unlock(previous);
}
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus) {
    if (id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6) return SPI_INVALID_ARGUMENT;
    if (id != EMELA_SPI_ID_SPI1) return SPI_UNSUPPORTED;
    *bus = &bus1;
    return SPI_OK;
}
