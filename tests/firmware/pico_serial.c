/* SPDX-License-Identifier: Apache-2.0 */
#include <assert.h>
#include <string.h>
#include "bsp.h"
#include "gpio_support.h"
#include "emela_uart.h"
#include "emelino_spi.h"
#include "emela_i2c.h"
#include "hardware/gpio.h"
#include "hardware/irq.h"
#include "hardware/uart.h"
#include "hardware/spi.h"
#include "hardware/i2c.h"

uint mock_core, mock_exception, mock_clock = 125000000;
uint32_t mock_irq_mask;
uart_hw_t mock_uart;
spi_hw_t mock_spi;
i2c_hw_t mock_i2c;
static uint64_t ticks;
static bool uart_readable, spi_readable = true, irq_enabled[32];
static bool output[32], latch[32], held_low[32];
static void (*handlers[32])(void);
static uint uart_resets, i2c_resets, scl_low_edges;
static uint spi_resets, spi_selected_clock_edges;
static uint functions[32];
static uint32_t inject_i2c_abort;
static uint64_t uart_read_resume_delay, pending_resume_delay;

void mock_interrupt_resume(void) {
    ticks += pending_resume_delay;
    pending_resume_delay = 0;
}

uint64_t emelino_pico_now_us(void) {
    ticks += 10;
    if (inject_i2c_abort && mock_i2c.data_cmd != 0xdead) {
        mock_i2c.raw_intr_stat = I2C_IC_RAW_INTR_STAT_TX_ABRT_BITS;
        mock_i2c.tx_abrt_source = inject_i2c_abort;
        inject_i2c_abort = 0;
        if (irq_enabled[I2C0_IRQ] && mock_i2c.intr_mask) handlers[I2C0_IRQ]();
    }
    return ticks;
}
bool emelino_pico_delay_us(uint64_t delay, uint64_t deadline) {
    ticks += delay;
    return ticks < deadline;
}
void gpio_disable_pulls(uint p) { (void)p; }
void gpio_pull_up(uint p) { (void)p; }
void gpio_set_function(uint p, uint fn) { functions[p] = fn; }
void gpio_put(uint p, bool high) { latch[p] = high; }
void gpio_set_dir(uint p, bool value) {
    if (p == 5 && value && !output[p]) ++scl_low_edges;
    output[p] = value;
}
bool gpio_get(uint p) { return !held_low[p] && (!output[p] || latch[p]); }
void irq_clear(uint irq) { (void)irq; }
void irq_set_priority(uint irq, uint8_t priority) { (void)irq; assert(priority == 0x40 || priority == 0x80 || priority == 0xc0); }
void irq_set_enabled(uint irq, bool enabled) { irq_enabled[irq] = enabled; }
void irq_set_exclusive_handler(uint irq, void (*handler)(void)) { handlers[irq] = handler; }
uint uart_init(uart_inst_t *p, uint baud) { (void)p; memset(&mock_uart, 0, sizeof(mock_uart)); ++uart_resets; return baud; }
void uart_set_fifo_enabled(uart_inst_t *p, bool enabled) { (void)p; assert(!enabled); }
void uart_set_format(uart_inst_t *p, uint data, uint stop, uart_parity_t parity) {
    (void)p; assert(data == 7 || data == 8); assert(stop == 1 || stop == 2); assert(parity <= UART_PARITY_ODD);
}
bool uart_is_readable(uart_inst_t *p) {
    (void)p;
    bool value = uart_readable;
    uart_readable = false;
    if (value) pending_resume_delay = uart_read_resume_delay;
    return value;
}
bool uart_is_writable(uart_inst_t *p) { (void)p; return true; }
uint spi_init(spi_inst_t *p, uint baud) {
    (void)p;
    /* SDK spi_init enables mode 0 before returning. A previously high SCK
       consequently falls while a caller-owned active-low CS is asserted. */
    if (functions[18] == GPIO_FUNC_SPI && output[17] && !latch[17]
        && (mock_spi.cr0 & SPI_SSPCR0_SPO_BITS)) ++spi_selected_clock_edges;
    ++spi_resets;
    memset(&mock_spi, 0, sizeof(mock_spi));
    return baud;
}
bool spi_is_writable(spi_inst_t *p) { (void)p; return true; }
bool spi_is_readable(spi_inst_t *p) { (void)p; return spi_readable; }
uint i2c_init(i2c_inst_t *p, uint baud) {
    (void)p; ++i2c_resets; memset(&mock_i2c, 0, sizeof(mock_i2c)); mock_i2c.enable = 1; return baud;
}
void i2c_deinit(i2c_inst_t *p) { (void)p; mock_i2c.enable = 0; }
int32_t emelino_pico_gpio_output_owned(uint16_t owner, int32_t pin, int32_t level) {
    int32_t status = emelino_pico_pins_claim(owner, &pin, 1);
    if (!status) { gpio_put(pin, level); gpio_set_dir(pin, true); }
    return status;
}
int32_t emelino_pico_gpio_write_owned(uint16_t owner, int32_t pin, int32_t level) {
    if (emelino_pico_pin_owner(pin) != owner) return 5;
    gpio_put(pin, level);
    return 0;
}
void emelino_pico_gpio_release_owned(uint16_t owner, int32_t pin) { emelino_pico_pins_release(owner, &pin, 1); }

static EmelaUartConfig uart_config(void) { return (EmelaUartConfig){115200, 1, 1, 0, 2, 0, 2}; }
static EmelaSpiConfig spi_config(int32_t rate) { return (EmelaSpiConfig){0, 0, 0, 0, 0, rate, 0}; }
static EmelaI2cConfig i2c_config(void) { return (EmelaI2cConfig){100000, 0}; }

static void uart_case(void) {
    assert(emela_uart_enable_interrupt(0, 1, 0) == 3);
    EmelaUartConfig config = uart_config();
    config.data_bits = 0; config.parity = 2; config.stop_bits = 3;
    assert(emela_uart_configure(0, config) == 0);
    assert(emelino_pico_pin_owner(0) == 2 && emelino_pico_pin_owner(1) == 2);
    uint16_t word = 128;
    assert(emela_uart_transmit(0, &word, 1, 1) == 1 && mock_uart.dr == 0);
    word = 127;
    assert(emela_uart_transmit(0, &word, 1, 1) == 0 && mock_uart.dr == 127);
    uint before = uart_resets;
    config.baud_rate = 1;
    assert(emela_uart_configure(0, config) == 4 && uart_resets == before);
    assert(emela_uart_transmit(0, &word, 1, 1) == 0);
    config = uart_config(); config.direction = 1;
    assert(emela_uart_configure(0, config) == 0);
    assert(emelino_pico_pin_owner(1) == 0 && functions[1] == GPIO_FUNC_NULL);
    int32_t rx = 1;
    assert(emelino_pico_pins_claim(1, &rx, 1) == 0);
    config.direction = 2; before = uart_resets;
    assert(emela_uart_configure(0, config) == 7 && uart_resets == before);
    uint16_t result, count = 99;
    assert(emela_uart_receive(0, 1, 1, &result, 1, &count) == 8 && count == 0);
    assert(emelino_pico_pin_owner(0) == 2 && emelino_pico_pin_owner(1) == 1);
}
static void uart_irq_case(void) {
    assert(emela_uart_configure(0, uart_config()) == 0);
    assert(emela_uart_set_receive_mode(0, 1) == 0);
    uint16_t values[64], length = 123;
    assert(emela_uart_receive(0, 1, 1, values, 64, &length) == 8 && length == 0);
    assert(emela_uart_enable_interrupt(0, 1, 1) == 0 && irq_enabled[UART0_IRQ]);
    for (uint32_t i = 0; i < 65; ++i) {
        mock_uart.dr = i; uart_readable = true;
        handlers[UART0_IRQ]();
    }
    assert(emela_uart_receive(0, 1, 1, values, 64, &length) == 11 && length == 0);
    assert(emela_uart_receive_some(0, 64, 20, values, 64, &length) == 0 && length == 64);
    for (uint16_t i = 0; i < length; ++i) assert(values[i] == i);
    mock_uart.dr = UART_UARTDR_PE_BITS | 42; uart_readable = true;
    handlers[UART0_IRQ]();
    assert(emela_uart_receive(0, 1, 1, values, 64, &length) == 9 && length == 0);
    int32_t discarded;
    assert(emela_uart_discard_input(0, 5, &discarded) == 0 && discarded == 1);
    assert(emela_uart_disable_interrupt(0) == 0 && !irq_enabled[UART0_IRQ]);
    assert(emela_uart_set_receive_mode(0, 0) == 0);
    mock_uart.dr = 77; uart_readable = true;
    uint64_t start = ticks;
    assert(emela_uart_receive(0, 2, 1, values, 64, &length) == 5 && length == 0);
    assert(ticks - start < 1100); /* One deadline, including the received prefix. */
    /* An unrelated long IRQ starts as the last RX critical section ends. The
       operation must recheck its deadline before publishing a successful read. */
    mock_uart.dr = 78; uart_readable = true; uart_read_resume_delay = 1001;
    assert(emela_uart_receive(0, 1, 1, values, 64, &length) == 5 && length == 0);
    uart_read_resume_delay = 0;
}
static void spi_case(void) {
    assert(emela_spi_configure(0, spi_config(1922)) == 4); /* Minimum is 1922.244 Hz. */
    assert(emelino_pico_pin_owner(18) == 0);
    assert(emela_spi_configure(0, spi_config(1923)) == 0);
    assert(mock_spi.cpsr == 254 && (mock_spi.cr0 >> 8) == 255);
    assert(emela_spi_configure(0, spi_config(976562)) == 0);
    uint32_t divider = mock_spi.cpsr * ((mock_spi.cr0 >> 8) + 1);
    assert(divider == 130); /* /128 is 976562.5, above the stated maximum. */
    assert((uint64_t)976562 * divider >= mock_clock);
    mock_clock = 150000000;
    assert(emela_spi_configure(0, spi_config(1000000)) == 0);
    divider = mock_spi.cpsr * ((mock_spi.cr0 >> 8) + 1);
    assert(divider == 150);
    EmelaSpiConfig invalid = spi_config(1000000); invalid.bit_order = 1;
    uint32_t old_cr = mock_spi.cr0;
    assert(emela_spi_configure(0, invalid) == 4 && mock_spi.cr0 == old_cr);
    uint16_t word = 256;
    assert(emela_spi_transmit(0, &word, 1, 1) == 1);
    spi_readable = false; word = 3;
    uint64_t start = ticks;
    assert(emela_spi_transmit(0, &word, 1, 1) == 5);
    assert(ticks - start < 1100 && mock_spi.cr1 == 0);
    assert(emela_spi_configure(0, spi_config(1000000)) == 8);
    EmelaSpiTransactionStatus recovered = emela_spi_recover(0);
    assert(!recovered.primary_spi && !recovered.primary_gpio && !recovered.cleanup_gpio);
    spi_readable = true;
    assert(emela_spi_transmit(0, &word, 1, 1) == 0);
}
static void spi_manual_cs_case(void) {
    const uint16_t words[] = {0x12, 0x34};
    gpio_set_function(17, GPIO_FUNC_SIO);
    gpio_put(17, true);
    gpio_set_dir(17, GPIO_OUT);
    for (unsigned phase = 0; phase < 2; ++phase) {
        EmelaSpiConfig config = spi_config(1000000);
        config.polarity = EMELA_SPI_CLOCK_POLARITY_IDLE_HIGH;
        config.phase = phase ? EMELA_SPI_CLOCK_PHASE_SECOND_EDGE : EMELA_SPI_CLOCK_PHASE_FIRST_EDGE;
        assert(emela_spi_configure(0, config) == 0);
        uint before = spi_resets;
        gpio_put(17, false);
        assert(emela_spi_transmit(0, words, 1, 1) == 0);
        assert(emela_spi_transmit(0, words + 1, 1, 1) == 0);
        assert(spi_resets == before && spi_selected_clock_edges == 0);
        gpio_put(17, true);

        /* Different requested limits may yield the same safe hardware rate. */
        config.max_frequency_hz++;
        assert(emela_spi_configure(0, config) == 0 && spi_resets == before);
        mock_clock = 150000000;
        assert(emela_spi_transmit(0, words, 1, 1) == 0);
        uint32_t divider = mock_spi.cpsr * ((mock_spi.cr0 >> 8) + 1);
        assert(divider == 150 && spi_resets == before + 1);
        before = spi_resets;
        EmelaSpiTransactionStatus recovered = emela_spi_recover(0);
        assert(!recovered.primary_spi && !recovered.primary_gpio && !recovered.cleanup_gpio);
        assert(spi_resets == before + 1);
        mock_clock = 125000000;
    }
}

static void i2c_case(void) {
    assert(emela_i2c_enable_interrupt(0, 1) == 3);
    assert(emela_i2c_configure(0, i2c_config()) == 0);
    uint before = i2c_resets;
    assert(emela_i2c_configure(0, (EmelaI2cConfig){1, 0}) == 4 && i2c_resets == before);
    mock_i2c.data_cmd = 0xdead;
    int32_t ready = 123;
    assert(emela_i2c_is_device_ready(0, 80, 3, 20, &ready) == 4 && ready == 0);
    assert(mock_i2c.data_cmd == 0xdead && i2c_resets == before);
    assert(emela_i2c_master_transmit(0, 80, NULL, 0, 20) == 4 && mock_i2c.data_cmd == 0xdead);
    uint16_t bad = 256, output_word, length = 99;
    assert(emela_i2c_master_transmit(0, 80, &bad, 1, 1) == 1);
    assert(emela_i2c_write_read(0, 80, NULL, 0, 0, 1, &output_word, 1, &length) == 1 && length == 0);
    held_low[5] = true;
    uint64_t start = ticks;
    assert(emela_i2c_recover(0, 1) == 5 && ticks - start < 1100);
    assert(!output[4] && !output[5] && !latch[4] && !latch[5]);
    assert(i2c_resets == before + 1 && functions[4] == GPIO_FUNC_I2C && functions[5] == GPIO_FUNC_I2C);
    held_low[5] = false; held_low[4] = true;
    assert(emela_i2c_recover(0, 5) == 10 && scl_low_edges == 10); /* Nine clocks and STOP. */
    assert(!output[4] && !output[5]);
    held_low[4] = false;
    assert(emela_i2c_recover(0, 5) == 0);
    assert(emela_i2c_enable_interrupt(0, 1) == 0 && handlers[I2C0_IRQ] && irq_enabled[I2C0_IRQ]);
    /* Inject an actual asynchronous abort after a command reaches hardware,
       then verify public error mapping and controller reinitialization. */
    mock_i2c.data_cmd = 0xdead; mock_i2c.status = I2C_IC_STATUS_TFNF_BITS;
    inject_i2c_abort = I2C_IC_TX_ABRT_SOURCE_ABRT_7B_ADDR_NOACK_BITS;
    uint16_t word = 1;
    before = i2c_resets;
    assert(emela_i2c_master_transmit(0, 80, &word, 1, 1) == 9 && i2c_resets == before + 1);
    assert(emela_i2c_disable_interrupt(0) == 0 && !irq_enabled[I2C0_IRQ] && !mock_i2c.intr_mask);
    mock_i2c.data_cmd = 0xdead; mock_i2c.status = I2C_IC_STATUS_TFNF_BITS;
    inject_i2c_abort = I2C_IC_TX_ABRT_SOURCE_ARB_LOST_BITS;
    assert(emela_i2c_master_transmit(0, 80, &word, 1, 1) == 11);
    mock_i2c.status = I2C_IC_STATUS_TFNF_BITS;
    start = ticks;
    assert(emela_i2c_write_read(0, 80, &word, 1, 1, 1, &output_word, 1, &length) == 5 && length == 0);
    assert(ticks - start < 1100 && !mock_i2c.intr_mask);
}
static void context_case(void) {
    for (unsigned kind = 0; kind < 3; ++kind) {
        mock_core = kind == 0; mock_exception = kind == 1 ? 20 : 0; mock_irq_mask = kind == 2;
        assert(emela_uart_configure(0, uart_config()) == 8);
        assert(emela_spi_configure(0, spi_config(1000000)) == 8);
        assert(emela_i2c_configure(0, i2c_config()) == 8);
        assert(mock_irq_mask == (kind == 2));
    }
    mock_core = mock_exception = mock_irq_mask = 0;
    assert(emela_uart_configure(0, uart_config()) == 0);
    assert(emela_spi_configure(0, spi_config(1000000)) == 0);
    assert(emela_i2c_configure(0, i2c_config()) == 0);
}
int main(int argc, char **argv) {
    assert(argc == 2);
    if (!strcmp(argv[1], "uart")) uart_case();
    else if (!strcmp(argv[1], "uart_irq")) uart_irq_case();
    else if (!strcmp(argv[1], "spi")) spi_case();
    else if (!strcmp(argv[1], "spi_manual_cs")) spi_manual_cs_case();
    else if (!strcmp(argv[1], "i2c")) i2c_case();
    else if (!strcmp(argv[1], "context")) context_case();
    else assert(false);
    return 0;
}
