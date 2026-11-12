/* SPDX-License-Identifier: Apache-2.0 */
#include <string.h>
#include <stdlib.h>
#include "board_config.h"
#include "bsp.h"
#include "emelino_spi.h"
#include "gpio_support.h"
#include "driver/spi_master.h"
#include "hal/spi_ll.h"

/* Logical Spi1 is SPI2_HOST. SPI0/1 are reserved for executable flash. */
static const int32_t pins[] = {EMELA_SPI1_SCK_PIN, EMELA_SPI1_MISO_PIN, EMELA_SPI1_MOSI_PIN};
static bool occupied, initialized, in_flight;
static spi_device_handle_t device;
/* A timed-out SDK transfer must never retain a pointer into a caller's stack. */
static spi_transaction_t transaction;
static EmelaSpiConfig active_config;

static int clock_for(int32_t maximum) {
    /* GPIO matrix routing is limited to 40 MHz. Stay below a requested cap
       even when the SDK chooses the closest divider by rounding upward. */
    if (maximum < 1000 || maximum > 40000000) return 0;
    int low = 1, high = maximum, selected = 0;
    while (low <= high) {
        int candidate = low + (high - low) / 2;
        int actual = spi_ll_master_cal_clock(80000000, candidate, 128, NULL);
        if (actual > 0 && (actual < maximum || (actual == maximum && 80000000 % maximum == 0))) {
            selected = candidate; low = candidate + 1;
        } else high = candidate - 1;
    }
    return selected;
}
static int32_t validate_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    (void)bus;
    if (config.mode < EMELA_SPI_MODE_MASTER || config.mode > EMELA_SPI_MODE_SLAVE
        || config.direction < EMELA_SPI_DIRECTION_TWO_LINE || config.direction > EMELA_SPI_DIRECTION_ONE_LINE
        || config.data_size < EMELA_SPI_DATA_SIZE_BITS8 || config.data_size > EMELA_SPI_DATA_SIZE_BITS16
        || config.polarity < EMELA_SPI_CLOCK_POLARITY_IDLE_LOW || config.polarity > EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH
        || config.phase < EMELA_SPI_CLOCK_PHASE_FIRST_EDGE || config.phase > EMELA_SPI_CLOCK_PHASE_SECOND_EDGE
        || config.bit_order < EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST
        || config.bit_order > EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST || config.max_frequency_hz <= 0)
        return SPI_INVALID_ARGUMENT;
    return config.mode != EMELA_SPI_MODE_MASTER || config.direction != EMELA_SPI_DIRECTION_TWO_LINE
        || !clock_for(config.max_frequency_hz) ? SPI_UNSUPPORTED : SPI_OK;
}
static bool same_config(EmelaSpiConfig a, EmelaSpiConfig b) {
    return a.mode == b.mode && a.direction == b.direction && a.data_size == b.data_size
        && a.polarity == b.polarity && a.phase == b.phase && a.bit_order == b.bit_order
        && a.max_frequency_hz == b.max_frequency_hz;
}
static int32_t finish_pending(void) {
    if (!in_flight) return SPI_OK;
    spi_transaction_t *completed = NULL;
    esp_err_t error = spi_device_get_trans_result(device, &completed, 0);
    if (error == ESP_ERR_TIMEOUT) return SPI_BUSY;
    if (error != ESP_OK || completed != &transaction) return SPI_OTHER;
    in_flight = false;
    return SPI_OK;
}
static int32_t apply_config(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    int32_t status = validate_config(bus, config);
    if (status) return status;
    status = finish_pending();
    if (status) return status;
    if (device && same_config(config, active_config)) return SPI_OK;
    bool created_bus = false;
    if (!initialized) {
        status = emelino_idf_pins_claim(EMELINO_IDF_OWNER_SPI2, pins, 3);
        if (status) return status == 5 ? SPI_RESOURCE_CONFLICT : SPI_INVALID_ARGUMENT;
        spi_bus_config_t native = {.mosi_io_num = EMELA_SPI1_MOSI_PIN, .miso_io_num = EMELA_SPI1_MISO_PIN,
            .sclk_io_num = EMELA_SPI1_SCK_PIN, .quadwp_io_num = -1, .quadhd_io_num = -1,
            .max_transfer_sz = 4};
        esp_err_t error = spi_bus_initialize(SPI2_HOST, &native, SPI_DMA_DISABLED);
        if (error != ESP_OK) {
            emelino_idf_pins_release(EMELINO_IDF_OWNER_SPI2, pins, 3);
            return error == ESP_ERR_INVALID_STATE ? SPI_RESOURCE_CONFLICT : SPI_OTHER;
        }
        initialized = true;
        created_bus = true;
    }
    if (device) {
        if (spi_bus_remove_device(device) != ESP_OK) return SPI_OTHER;
        device = NULL;
    }
    spi_device_interface_config_t native = {.clock_speed_hz = clock_for(config.max_frequency_hz),
        .mode = (config.polarity == EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH ? 2 : 0)
            | (config.phase == EMELA_SPI_CLOCK_PHASE_SECOND_EDGE ? 1 : 0),
        .spics_io_num = -1, .queue_size = 1,
        .flags = config.bit_order == EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST ? SPI_DEVICE_BIT_LSBFIRST : 0};
    if (spi_bus_add_device(SPI2_HOST, &native, &device) != ESP_OK) {
        if (created_bus) {
            if (spi_bus_free(SPI2_HOST) == ESP_OK) {
                initialized = false;
                emelino_idf_pins_release(EMELINO_IDF_OWNER_SPI2, pins, 3);
            } else {
                /* Retain a valid recovery configuration if native cleanup
                   itself failed before the first public configure succeeded. */
                bus->current_config = config;
            }
        }
        bus->recovery_required = initialized;
        return SPI_OTHER;
    }
    active_config = config;
    return SPI_OK;
}
static int32_t exchange(EmelinoSpiBus *bus, int32_t operation, const uint16_t *data,
                        uint16_t length, uint16_t *output, uint64_t deadline) {
    bool wide = active_config.data_size == EMELA_SPI_DATA_SIZE_BITS16;
    bool lsb = active_config.bit_order == EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST;
    for (uint16_t i = 0; i < length; ++i) {
        if (emelino_idf_now_us() >= deadline) return SPI_TIMEOUT;
        uint16_t word = operation == EMELA_SPI_OPERATION_READ ? (wide ? 65535 : 255) : data[i];
        memset(&transaction, 0, sizeof(transaction));
        transaction.flags = SPI_TRANS_USE_TXDATA | SPI_TRANS_USE_RXDATA;
        transaction.length = transaction.rxlength = wide ? 16 : 8;
        transaction.tx_data[0] = wide && !lsb ? (uint8_t)(word >> 8) : (uint8_t)word;
        transaction.tx_data[1] = lsb ? (uint8_t)(word >> 8) : (uint8_t)word;
        /* IDF polling_start only accepts an infinite wait. Queueing with a
           zero wait and checking completion keeps the public deadline bounded. */
        esp_err_t error = spi_device_queue_trans(device, &transaction, 0);
        if (error != ESP_OK) {
            bus->recovery_required = true;
            return error == ESP_ERR_TIMEOUT ? SPI_TIMEOUT : SPI_OTHER;
        }
        in_flight = true;
        int32_t status;
        while ((status = finish_pending()) == SPI_BUSY) {
            if (emelino_idf_now_us() >= deadline) {
                bus->recovery_required = true;
                return SPI_TIMEOUT;
            }
            emelino_idf_poll_wait(deadline);
        }
        if (status) { bus->recovery_required = true; return status; }
        if (emelino_idf_now_us() >= deadline) return SPI_TIMEOUT;
        if (output) output[i] = !wide ? transaction.rx_data[0] : lsb
            ? (uint16_t)(transaction.rx_data[0] | ((uint16_t)transaction.rx_data[1] << 8))
            : (uint16_t)(((uint16_t)transaction.rx_data[0] << 8) | transaction.rx_data[1]);
    }
    return SPI_OK;
}
static int32_t recover(EmelinoSpiBus *bus, EmelaSpiConfig config) {
    if (!initialized) return SPI_NOT_CONFIGURED;
    return apply_config(bus, config);
}
static int32_t cs_configure(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return emelino_idf_gpio_output_owned(EMELINO_IDF_OWNER_CS_BASE + slot, pin, level);
}
static int32_t cs_write(EmelinoSpiBus *bus, uint16_t slot, int32_t pin, int32_t level) {
    (void)bus;
    return emelino_idf_gpio_write_owned(EMELINO_IDF_OWNER_CS_BASE + slot, pin, level);
}
static void cs_release(EmelinoSpiBus *bus, uint16_t slot, int32_t pin) {
    (void)bus;
    emelino_idf_gpio_release_owned(EMELINO_IDF_OWNER_CS_BASE + slot, pin);
}
static uint64_t now_us(EmelinoSpiBus *bus) { (void)bus; return emelino_idf_now_us(); }
static bool delay_us(EmelinoSpiBus *bus, uint64_t delay, uint64_t deadline) {
    (void)bus; return emelino_idf_delay_us(delay, deadline);
}
static const EmelinoSpiOps operations = {.validate_config = validate_config, .configure = apply_config,
    .exchange = exchange, .recover = recover, .cs_configure = cs_configure, .cs_write = cs_write,
    .cs_release = cs_release, .now_us = now_us, .delay_us = delay_us};
static EmelinoSpiBus bus1 = {.ops = &operations};
int32_t emelino_spi_lock(void) {
    if (!emelino_idf_task_context()) return SPI_INVALID_STATE;
    uint32_t previous = emelino_idf_lock();
    int32_t status = occupied ? SPI_BUSY : SPI_OK;
    if (!status) occupied = true;
    emelino_idf_unlock(previous);
    return status;
}
void emelino_spi_unlock(void) {
    uint32_t previous = emelino_idf_lock(); occupied = false; emelino_idf_unlock(previous);
}
int32_t emelino_spi_bus(EmelaSpiId id, EmelinoSpiBus **bus) {
    if (id < EMELA_SPI_ID_SPI1 || id > EMELA_SPI_ID_SPI6) return SPI_INVALID_ARGUMENT;
    if (id != EMELA_SPI_ID_SPI1) return SPI_UNSUPPORTED;
    *bus = &bus1;
    return SPI_OK;
}
