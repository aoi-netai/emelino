/* SDK boundary tests: no physical MCU is represented by this fixture. */
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "esp_idf_mock.h"
#include "bsp.h"
#include "emela_gpio.h"
#include "emela_uart.h"
#include "emela_i2c.h"
#include "emelino_spi.h"

bool mock_isr;
static uint32_t mask;
static uint64_t now;
static unsigned live_allocations;
static bool fail_allocation;
void *__real_malloc(size_t size);
void __real_free(void *pointer);
void *__wrap_malloc(size_t size) {
    if (fail_allocation) return NULL;
    void *p = __real_malloc(size);
    if (p) ++live_allocations;
    return p;
}
void __wrap_free(void *p) { if (p) { assert(live_allocations); --live_allocations; } __real_free(p); }
uint32_t mock_lock(void) { uint32_t previous = mask; mask = 1; return previous; }
void mock_unlock(uint32_t previous) { mask = previous; }
int64_t esp_timer_get_time(void) { return (int64_t)now++; }
void esp_rom_delay_us(unsigned delay) { now += delay; }
static void fire_uart(void);
void vTaskDelay(unsigned ticks) { now += ticks * 1000; fire_uart(); }
static void (*uart_handler)(void *);
static void *uart_argument;
static bool tx_stuck, spi_stuck;
static void fire_uart(void) {
    if (!mask && uart_handler && (mock_uart.raw & mock_uart.enabled)) {
        mock_isr = true; uart_handler(uart_argument); mock_isr = false;
    }
}
void taskYIELD(void) { now += 10; fire_uart(); }

static int levels[22], modes[22], pulls[22];
static bool gpio_enabled[22];
static void (*gpio_handlers[22])(void *);
static void *gpio_arguments[22];
static unsigned callbacks;
esp_err_t gpio_set_level(int pin, int level) { assert(pin >= 0 && pin < 22); levels[pin] = level; return ESP_OK; }
esp_err_t gpio_config(const gpio_config_t *config) {
    for (unsigned pin = 0; pin < 22; ++pin) if (config->pin_bit_mask & (UINT64_C(1) << pin)) {
        modes[pin] = config->mode; pulls[pin] = config->pull_up_en - config->pull_down_en;
    }
    return ESP_OK;
}
esp_err_t gpio_reset_pin(int pin) { modes[pin] = GPIO_MODE_DISABLE; return ESP_OK; }
int gpio_get_level(int pin) { return levels[pin]; }
esp_err_t gpio_install_isr_service(int flags) { return ESP_OK; }
void gpio_uninstall_isr_service(void) {}
esp_err_t gpio_intr_disable(int pin) { gpio_enabled[pin] = false; return ESP_OK; }
esp_err_t gpio_intr_enable(int pin) { gpio_enabled[pin] = true; return ESP_OK; }
esp_err_t gpio_set_intr_type(int pin, gpio_int_type_t type) { return ESP_OK; }
esp_err_t gpio_isr_handler_add(int pin, void (*handler)(void *), void *argument) {
    gpio_handlers[pin] = handler; gpio_arguments[pin] = argument; return ESP_OK;
}
esp_err_t gpio_isr_handler_remove(int pin) { gpio_handlers[pin] = NULL; return ESP_OK; }
void emela_gpio_dispatch_interrupt(EmelaGpioPin pin, EmelaGpioPinState state) {
    assert(mock_isr && pin.id == 9 && state == EMELA_GPIO_PIN_STATE_HIGH); ++callbacks;
}

uart_dev_t mock_uart;
static uint8_t uart_tx[512];
static unsigned uart_tx_length;
esp_err_t esp_intr_alloc(int source, int flags, void (*handler)(void *), void *argument, intr_handle_t *handle) {
    uart_handler = handler; uart_argument = argument; *handle = (void *)1; return ESP_OK;
}
esp_err_t esp_intr_free(intr_handle_t handle) { uart_handler = NULL; return ESP_OK; }
bool uart_is_driver_installed(int port) { return false; }
esp_err_t uart_param_config(int port, const uart_config_t *config) { return ESP_OK; }
esp_err_t uart_set_pin(int port, int tx, int rx, int rts, int cts) { return ESP_OK; }
void uart_ll_disable_intr_mask(uart_dev_t *hw, uint32_t bits) { hw->enabled &= ~bits; }
void uart_ll_ena_intr_mask(uart_dev_t *hw, uint32_t bits) { hw->enabled |= bits; }
void uart_ll_clr_intsts_mask(uart_dev_t *hw, uint32_t bits) { hw->raw &= ~bits; }
uint32_t uart_ll_get_intsts_mask(uart_dev_t *hw) { return hw->raw & hw->enabled; }
uint32_t uart_ll_get_intraw_mask(uart_dev_t *hw) { return hw->raw; }
unsigned uart_ll_get_rxfifo_len(uart_dev_t *hw) { return hw->rx_length; }
unsigned uart_ll_get_txfifo_len(uart_dev_t *hw) { return tx_stuck ? 0 : 128; }
void uart_ll_read_rxfifo(uart_dev_t *hw, uint8_t *out, uint32_t length) {
    assert(length <= hw->rx_length); memcpy(out, hw->rx, length); hw->rx_length -= length;
    memmove(hw->rx, hw->rx + length, hw->rx_length);
}
void uart_ll_write_txfifo(uart_dev_t *hw, const uint8_t *data, uint32_t length) {
    assert(uart_tx_length + length <= sizeof(uart_tx)); memcpy(uart_tx + uart_tx_length, data, length);
    uart_tx_length += length; hw->raw |= UART_INTR_TX_DONE;
}
void uart_ll_rxfifo_rst(uart_dev_t *hw) { hw->rx_length = 0; }
void uart_ll_txfifo_rst(uart_dev_t *hw) {}
void uart_ll_set_rxfifo_full_thr(uart_dev_t *hw, unsigned threshold) {}
void uart_ll_set_rx_tout(uart_dev_t *hw, unsigned timeout) {}
bool uart_ll_is_tx_idle(uart_dev_t *hw) { return !tx_stuck; }
static void inject_uart(unsigned count, bool interrupt) {
    assert(count <= sizeof(mock_uart.rx));
    for (unsigned i = 0; i < count; ++i) mock_uart.rx[i] = (uint8_t)(i + 1);
    mock_uart.rx_length = count; mock_uart.raw |= UART_INTR_RXFIFO_FULL;
    if (interrupt) fire_uart();
}

static spi_device_interface_config_t spi_config;
static spi_transaction_t *pending_spi;
static uint8_t spi_tx[4];
static unsigned spi_bits;
static bool fail_spi_device;
int spi_ll_master_cal_clock(int clock, int requested, int duty, void *result) {
    unsigned divisor = ((unsigned)clock + (unsigned)requested / 2) / (unsigned)requested;
    if (!divisor) divisor = 1;
    return clock / (int)divisor;
}
esp_err_t spi_bus_initialize(int host, const spi_bus_config_t *config, int dma) {
    assert(host == SPI2_HOST && config->sclk_io_num == 4 && dma == SPI_DMA_DISABLED); return ESP_OK;
}
esp_err_t spi_bus_free(int host) { return ESP_OK; }
esp_err_t spi_bus_remove_device(spi_device_handle_t device) { assert(!pending_spi); return ESP_OK; }
esp_err_t spi_bus_add_device(int host, const spi_device_interface_config_t *config, spi_device_handle_t *device) {
    if (fail_spi_device) return ESP_ERR_NO_MEM;
    spi_config = *config; *device = (void *)1; return ESP_OK;
}
esp_err_t spi_device_queue_trans(spi_device_handle_t device, spi_transaction_t *transaction, unsigned timeout) {
    assert(!pending_spi); pending_spi = transaction; spi_bits = transaction->length;
    memcpy(spi_tx, transaction->tx_data, sizeof(spi_tx)); return ESP_OK;
}
esp_err_t spi_device_get_trans_result(spi_device_handle_t device, spi_transaction_t **transaction, unsigned timeout) {
    assert(pending_spi);
    if (spi_stuck) return ESP_ERR_TIMEOUT;
    memcpy(pending_spi->rx_data, pending_spi->tx_data, sizeof(pending_spi->rx_data));
    *transaction = pending_spi;
    pending_spi = NULL; return ESP_OK;
}

static uint8_t i2c_tx[512];
static size_t i2c_tx_length;
static uint16_t i2c_address;
static unsigned i2c_added, i2c_removed, repeated_starts, resets;
static esp_err_t i2c_error;
static uint64_t probe_delay;
esp_err_t i2c_new_master_bus(const i2c_master_bus_config_t *config, i2c_master_bus_handle_t *bus) {
    assert(config->sda_io_num == 0 && config->scl_io_num == 1); *bus = (void *)1; return ESP_OK;
}
esp_err_t i2c_master_probe(i2c_master_bus_handle_t bus, uint16_t address, int timeout) {
    assert(timeout > 0); now += probe_delay; return address == 0x50 ? ESP_OK : ESP_ERR_NOT_FOUND;
}
esp_err_t i2c_master_bus_add_device(i2c_master_bus_handle_t bus, const i2c_device_config_t *config, i2c_master_dev_handle_t *device) {
    i2c_address = config->device_address; ++i2c_added; *device = (void *)2; return ESP_OK;
}
esp_err_t i2c_master_bus_rm_device(i2c_master_dev_handle_t device) { ++i2c_removed; return ESP_OK; }
esp_err_t i2c_master_transmit(i2c_master_dev_handle_t device, const uint8_t *data, size_t length, int timeout) {
    assert(timeout > 0 && length <= sizeof(i2c_tx));
    memcpy(i2c_tx, data, length); i2c_tx_length = length;
    return i2c_error ? i2c_error : i2c_address == 0x50 ? ESP_OK : ESP_ERR_INVALID_RESPONSE;
}
esp_err_t i2c_master_receive(i2c_master_dev_handle_t device, uint8_t *data, size_t length, int timeout) {
    assert(timeout > 0);
    for (size_t i = 0; i < length; ++i) data[i] = (uint8_t)(0xa0 + i);
    return i2c_error;
}
esp_err_t i2c_master_transmit_receive(i2c_master_dev_handle_t device, const uint8_t *tx, size_t tx_length,
                                     uint8_t *rx, size_t rx_length, int timeout) {
    ++repeated_starts;
    esp_err_t error = i2c_master_transmit(device, tx, tx_length, timeout);
    return error ? error : i2c_master_receive(device, rx, rx_length, timeout);
}
esp_err_t i2c_master_bus_reset(i2c_master_bus_handle_t bus) { ++resets; return ESP_OK; }

static EmelaGpioConfig output_config(void) {
    return (EmelaGpioConfig){.tag = EMELA_GPIO_CONFIG_OUTPUT_OPEN_DRAIN,
        .payload.output_open_drain = {.pull = EMELA_GPIO_PULL_UP, .initial = EMELA_GPIO_PIN_STATE_HIGH}};
}
static EmelaUartConfig uart_config(void) {
    return (EmelaUartConfig){.baud_rate = 115200, .data_bits = EMELA_UART_DATA_BITS_BITS8,
        .stop_bits = EMELA_UART_STOP_BITS_ONE, .parity = EMELA_UART_PARITY_NO_PARITY,
        .direction = EMELA_UART_DIRECTION_BOTH, .flow_control = EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL,
        .oversampling = EMELA_UART_OVERSAMPLING_NATIVE};
}
static EmelaSpiConfig spi_word_config(bool wide, bool lsb) {
    return (EmelaSpiConfig){.mode = EMELA_SPI_MODE_MASTER, .direction = EMELA_SPI_DIRECTION_TWO_LINE,
        .data_size = wide ? EMELA_SPI_DATA_SIZE_BITS16 : EMELA_SPI_DATA_SIZE_BITS8,
        .polarity = EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH, .phase = EMELA_SPI_CLOCK_PHASE_SECOND_EDGE,
        .max_frequency_hz = 1100000, .bit_order = lsb ? EMELA_SPI_BIT_ORDER_LEAST_SIGNIFICANT_FIRST
            : EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST};
}
static void configure_i2c(void) {
    assert(emela_i2c_configure(0, (EmelaI2cConfig){100000, EMELA_I2C_DUTY_CYCLE_STANDARD}) == 0);
}
static void test_gpio(void) {
    EmelaGpioPin pin = {.id = 7};
    assert(emela_gpio_configure(pin, output_config()) == 0);
    assert(modes[7] == GPIO_MODE_INPUT_OUTPUT_OD && levels[7] == 1 && pulls[7] == 1);
    levels[7] = 0; /* Another open-drain device holds the released line LOW. */
    assert(emela_gpio_toggle(pin) == 0 && levels[7] == 0);
    assert(emela_gpio_toggle(pin) == 0 && levels[7] == 1);
    assert(emela_gpio_configure((EmelaGpioPin){12}, output_config()) == 1);
    EmelaGpioConfig input = {.tag = EMELA_GPIO_CONFIG_INPUT, .payload.input = {.pull = EMELA_GPIO_PULL_UP}};
    assert(emela_gpio_configure((EmelaGpioPin){9}, input) == 0);
    assert(emela_gpio_enable_interrupt((EmelaGpioPin){9}, EMELA_GPIO_EDGE_BOTH, EMELA_INTERRUPT_PRIORITY_NORMAL) == 0);
    assert(emela_gpio_configure((EmelaGpioPin){9}, input) == 4);
    levels[9] = 1; mock_isr = true; gpio_handlers[9](gpio_arguments[9]); mock_isr = false;
    assert(callbacks == 1 && gpio_enabled[9]);
    assert(emela_gpio_disable_interrupt((EmelaGpioPin){9}) == 0 && !gpio_enabled[9]);
    assert(emela_gpio_configure((EmelaGpioPin){9}, input) == 0);
}
static void test_ownership(void) {
    assert(emela_gpio_configure((EmelaGpioPin){5}, output_config()) == 0);
    assert(emela_spi_configure(0, spi_word_config(false, false)) == SPI_RESOURCE_CONFLICT);
    assert(!emelino_idf_pin_owner(4) && !emelino_idf_pin_owner(6));
    assert(emela_uart_configure(0, uart_config()) == 0);
    assert(emela_gpio_configure((EmelaGpioPin){21}, output_config()) == 5);
    EmelaUartConfig receive = uart_config(); receive.direction = EMELA_UART_DIRECTION_RECEIVE;
    assert(emela_uart_configure(0, receive) == 0);
    assert(!emelino_idf_pin_owner(21) && emelino_idf_pin_owner(20));
    assert(emela_gpio_configure((EmelaGpioPin){21}, output_config()) == 0);
}
static void test_uart(bool interrupt) {
    assert(emela_uart_configure(0, uart_config()) == 0);
    uint16_t data[] = {0, 0x80, 0xff}, out[64], length;
    if (interrupt) {
        assert(emela_uart_enable_interrupt(0, EMELA_INTERRUPT_PRIORITY_NORMAL, 1) == 0);
        assert(emela_uart_set_receive_mode(0, EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) == 0);
    }
    assert(emela_uart_transmit(0, data, 3, 20) == 0);
    assert(uart_tx_length == 3 && !memcmp(uart_tx, (uint8_t[]){0, 0x80, 0xff}, 3));
    uint16_t invalid[] = {256};
    assert(emela_uart_transmit(0, invalid, 1, 20) == 1 && uart_tx_length == 3);
    inject_uart(interrupt ? 64 : 3, interrupt);
    assert(emela_uart_receive(0, interrupt ? 64 : 3, 20, out, 64, &length) == 0);
    assert(length == (interrupt ? 64 : 3) && out[0] == 1 && out[length - 1] == length);
    if (interrupt) {
        inject_uart(65, true);
        assert(emela_uart_receive(0, 1, 20, out, 64, &length) == 11 && length == 0);
        int32_t discarded;
        assert(emela_uart_discard_input(0, 64, &discarded) == 0 && discarded == 64);
        inject_uart(1, true);
        assert(emela_uart_receive_some(0, 10, 20, out, 64, &length) == 0 && length == 1);
        assert(emela_uart_disable_interrupt(0) == 0);
        assert(emela_uart_set_receive_mode(0, EMELA_UART_RECEIVE_MODE_POLLING) == 0);
    }
    assert(emela_uart_receive(0, 1, 1, out, 64, &length) == 5 && length == 0);
    tx_stuck = true;
    assert(emela_uart_transmit(0, data, 3, 1) == 5);
    tx_stuck = false;
    assert(emela_uart_transmit(0, data, 3, 20) == 0);
    assert(emela_uart_configure(1, uart_config()) == 4);
}
static void test_spi(bool timeout) {
    EmelaSpiConfig config = spi_word_config(true, false);
    assert(emela_spi_configure(0, config) == 0);
    assert(spi_ll_master_cal_clock(80000000, spi_config.clock_speed_hz, 128, NULL) <= config.max_frequency_hz);
    uint16_t data[] = {0x1234, 0xabcd}, out[2], length;
    assert(emela_spi_transfer(0, data, 2, 20, out, 2, &length) == 0);
    assert(length == 2 && out[0] == 0x1234 && out[1] == 0xabcd && spi_bits == 16);
    assert(spi_tx[0] == 0xab && spi_tx[1] == 0xcd);
    config = spi_word_config(true, true);
    assert(emela_spi_configure(0, config) == 0);
    assert(emela_spi_transfer(0, data, 1, 20, out, 2, &length) == 0 && out[0] == 0x1234);
    assert(spi_tx[0] == 0x34 && spi_tx[1] == 0x12 && spi_config.flags == SPI_DEVICE_BIT_LSBFIRST);
    EmelaSpiDevice device;
    EmelaSpiDeviceConfig device_config = {.bus = 0, .config = config, .cs = {10}, .active = 0, .setup_us = 1, .hold_us = 1};
    EmelaSpiTransactionStatus status = emela_spi_create_device(device_config, &device);
    assert(!status.primary_spi && !status.primary_gpio);
    EmelaSpiOperation operation = {.tag = EMELA_SPI_OPERATION_TRANSFER, .data = data, .length = 2};
    spi_stuck = timeout;
    status = emela_spi_transaction(device, &operation, 1, timeout ? 1 : 20, out, 2, &length);
    assert(levels[10] == 1); /* CS is released on success and timeout. */
    assert(status.primary_spi == (timeout ? SPI_TIMEOUT : SPI_OK) && !status.cleanup_gpio);
    if (timeout) {
        assert(pending_spi && pending_spi->tx_data[0] == 0x34);
        status = emela_spi_recover(0);
        assert(status.primary_spi == SPI_BUSY);
        spi_stuck = false;
        status = emela_spi_recover(0);
        assert(!status.primary_spi && !pending_spi);
        assert(emela_spi_transfer(0, data, 1, 20, out, 2, &length) == 0);
    }
}
static void test_spi_config_failure(void) {
    fail_spi_device = true;
    assert(emela_spi_configure(0, spi_word_config(false, false)) == SPI_OTHER);
    assert(!emelino_idf_pin_owner(4) && !emelino_idf_pin_owner(5) && !emelino_idf_pin_owner(6));
    fail_spi_device = false;
    assert(emela_spi_configure(0, spi_word_config(false, false)) == SPI_OK);
}
static void test_i2c(bool failure) {
    configure_i2c();
    uint16_t prefix[] = {0xd0, 0x12}, out[2], length;
    assert(emela_i2c_write_read(0, 0x50, prefix, 2, 2, 20, out, 2, &length) == 0);
    assert(length == 2 && out[0] == 0xa0 && out[1] == 0xa1 && repeated_starts == 1);
    assert(i2c_tx_length == 2 && i2c_tx[0] == 0xd0 && i2c_tx[1] == 0x12);
    assert(emela_i2c_memory_write(0, 0x50, 0x1234, EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS16, prefix, 2, 20) == 0);
    assert(i2c_tx_length == 4 && !memcmp(i2c_tx, (uint8_t[]){0x12, 0x34, 0xd0, 0x12}, 4));
    int32_t ready;
    assert(emela_i2c_is_device_ready(0, 0x50, 2, 20, &ready) == 0 && ready == 1);
    assert(emela_i2c_is_device_ready(0, 0x51, 2, 20, &ready) == 0 && ready == 0);
    assert(emela_i2c_enable_interrupt(0, EMELA_INTERRUPT_PRIORITY_NORMAL) == 4);
    assert(emela_i2c_recover(0, 20) == 0 && resets == 1);
    if (failure) {
        probe_delay = 50000;
        assert(emela_i2c_is_device_ready(0, 0x50, 1, 20, &ready) == 5 && ready == 0);
        probe_delay = 0;
        i2c_error = ESP_ERR_TIMEOUT;
        assert(emela_i2c_write_read(0, 0x50, prefix, 2, 2, 20, out, 2, &length) == 5 && length == 0);
        i2c_error = ESP_ERR_INVALID_RESPONSE;
        assert(emela_i2c_master_transmit(0, 0x50, prefix, 2, 20) == 9);
        i2c_error = ESP_OK;
        fail_allocation = true;
        assert(emela_i2c_write_read(0, 0x50, prefix, 2, 2, 20, out, 2, &length) == 2 && length == 0);
        fail_allocation = false;
        assert(emela_i2c_master_transmit(0, 0x50, (uint16_t[]){256}, 1, 20) == 1);
        assert(emela_i2c_master_transmit(0, 0x50, prefix, 2, 20) == 0);
    }
    assert(i2c_added == i2c_removed && live_allocations == 0);
}
static void test_context(void) {
    mock_isr = true;
    assert(emela_gpio_configure((EmelaGpioPin){7}, output_config()) == 6);
    assert(emela_uart_configure(0, uart_config()) == 8);
    assert(emela_spi_configure(0, spi_word_config(false, false)) == SPI_INVALID_STATE);
    assert(emela_i2c_configure(0, (EmelaI2cConfig){100000, EMELA_I2C_DUTY_CYCLE_STANDARD}) == 8);
    mock_isr = false;
    assert(!mask && !live_allocations);
}
int main(int argc, char **argv) {
    assert(argc == 2);
    if (!strcmp(argv[1], "gpio")) test_gpio();
    else if (!strcmp(argv[1], "ownership")) test_ownership();
    else if (!strcmp(argv[1], "uart")) test_uart(false);
    else if (!strcmp(argv[1], "uart_irq")) test_uart(true);
    else if (!strcmp(argv[1], "spi")) test_spi(false);
    else if (!strcmp(argv[1], "spi_timeout")) test_spi(true);
    else if (!strcmp(argv[1], "spi_config_failure")) test_spi_config_failure();
    else if (!strcmp(argv[1], "i2c")) test_i2c(false);
    else if (!strcmp(argv[1], "i2c_failure")) test_i2c(true);
    else if (!strcmp(argv[1], "context")) test_context();
    else abort();
    assert(!mask && !live_allocations);
    printf("PASS %s\n", argv[1]);
}
