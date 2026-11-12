/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emela_uart.h"
#include "driver/gpio.h"
#include "driver/uart.h"
#include "hal/uart_ll.h"
#include "esp_intr_alloc.h"
#include "soc/interrupts.h"

enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, PARITY, FRAMING, OVERRUN, OTHER };
#define HW UART_LL_GET_HW(UART_NUM_0)
#define RX_CAPACITY 64
#define RX_ERRORS (UART_INTR_PARITY_ERR | UART_INTR_FRAM_ERR | UART_INTR_RXFIFO_OVF | UART_INTR_BRK_DET)
#define RX_INTERRUPTS (RX_ERRORS | UART_INTR_RXFIFO_FULL | UART_INTR_RXFIFO_TOUT)
static bool configured, occupied, can_transmit, can_receive, notify_transmit;
static intr_handle_t interrupt_handle;
static EmelaUartReceiveMode receive_mode;
static uint16_t data_mask;
static volatile uint16_t read_index, received_count;
static uint16_t received[RX_CAPACITY];
static volatile uint32_t receive_errors;
static volatile bool tx_done;
static int32_t claimed_pins[2];
static uint16_t claimed_count;

static int32_t enter(EmelaUartId id, bool require_configured) {
    if (id < EMELA_UART_ID_UART0 || id > EMELA_UART_ID_UART7) return INVALID_ARGUMENT;
    if (id != EMELA_UART_ID_UART0) return UNSUPPORTED;
    if (!emelino_idf_task_context()) return INVALID_STATE;
    uint32_t previous = emelino_idf_lock();
    int32_t status = require_configured && !configured ? NOT_CONFIGURED : occupied ? BUSY : OK;
    if (!status) occupied = true;
    emelino_idf_unlock(previous);
    return status;
}
static int32_t leave(int32_t status) {
    uint32_t previous = emelino_idf_lock(); occupied = false; emelino_idf_unlock(previous);
    return status;
}
static int32_t error_status(uint32_t errors) {
    return errors & UART_INTR_RXFIFO_OVF ? OVERRUN : errors & UART_INTR_PARITY_ERR ? PARITY
        : errors & UART_INTR_FRAM_ERR ? FRAMING : errors ? OTHER : OK;
}
static void receive_interrupts(void) {
    uart_ll_disable_intr_mask(HW, RX_INTERRUPTS);
    if (interrupt_handle && can_receive && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        uart_ll_ena_intr_mask(HW, RX_INTERRUPTS);
}
static void uart_interrupt(void *argument) {
    (void)argument;
    uint32_t events = uart_ll_get_intsts_mask(HW);
    if (events & RX_INTERRUPTS) {
        receive_errors |= events & RX_ERRORS;
        /* Bound work to one FIFO, even under continuous serial input. */
        for (unsigned i = 0; i < UART_LL_FIFO_DEF_LEN && uart_ll_get_rxfifo_len(HW); ++i) {
            uint8_t word;
            uart_ll_read_rxfifo(HW, &word, 1);
            if (received_count == RX_CAPACITY) receive_errors |= UART_INTR_RXFIFO_OVF;
            else {
                received[(read_index + received_count) % RX_CAPACITY] = word & data_mask;
                ++received_count;
            }
        }
        uart_ll_clr_intsts_mask(HW, RX_INTERRUPTS);
    }
    if (events & UART_INTR_TX_DONE) {
        uart_ll_disable_intr_mask(HW, UART_INTR_TX_DONE);
        uart_ll_clr_intsts_mask(HW, UART_INTR_TX_DONE);
        tx_done = true;
    }
}
int32_t emela_uart_configure(EmelaUartId id, EmelaUartConfig config) {
    if (config.baud_rate <= 0 || config.data_bits < EMELA_UART_DATA_BITS_BITS7
        || config.data_bits > EMELA_UART_DATA_BITS_BITS9 || config.stop_bits < EMELA_UART_STOP_BITS_HALF
        || config.stop_bits > EMELA_UART_STOP_BITS_TWO || config.parity < EMELA_UART_PARITY_NO_PARITY
        || config.parity > EMELA_UART_PARITY_ODD || config.direction < EMELA_UART_DIRECTION_RECEIVE
        || config.direction > EMELA_UART_DIRECTION_BOTH || config.flow_control < EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.flow_control > EMELA_UART_FLOW_CONTROL_RTS_CTS || config.oversampling < EMELA_UART_OVERSAMPLING_BY8
        || config.oversampling > EMELA_UART_OVERSAMPLING_NATIVE) return INVALID_ARGUMENT;
    if (config.baud_rate < 300 || config.baud_rate > 5000000 || config.data_bits == EMELA_UART_DATA_BITS_BITS9
        || (config.stop_bits != EMELA_UART_STOP_BITS_ONE && config.stop_bits != EMELA_UART_STOP_BITS_TWO)
        || config.flow_control != EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.oversampling != EMELA_UART_OVERSAMPLING_NATIVE) return UNSUPPORTED;
    int32_t status = enter(id, false);
    if (status) return status;
    if (uart_is_driver_installed(UART_NUM_0)) return leave(RESOURCE_CONFLICT);
    bool tx = config.direction != EMELA_UART_DIRECTION_RECEIVE;
    bool rx = config.direction != EMELA_UART_DIRECTION_TRANSMIT;
    int32_t pins[2]; uint16_t count = 0;
    if (tx) pins[count++] = EMELA_UART0_TX_PIN;
    if (rx) pins[count++] = EMELA_UART0_RX_PIN;
    status = emelino_idf_pins_claim(EMELINO_IDF_OWNER_UART0, pins, count);
    if (status) return leave(status == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
    if (interrupt_handle) {
        uart_ll_disable_intr_mask(HW, UINT32_MAX);
        esp_intr_free(interrupt_handle); interrupt_handle = NULL;
    }
    uart_config_t native = {.baud_rate = config.baud_rate,
        .data_bits = config.data_bits == EMELA_UART_DATA_BITS_BITS7 ? UART_DATA_7_BITS : UART_DATA_8_BITS,
        .parity = config.parity == EMELA_UART_PARITY_EVEN ? UART_PARITY_EVEN
            : config.parity == EMELA_UART_PARITY_ODD ? UART_PARITY_ODD : UART_PARITY_DISABLE,
        .stop_bits = config.stop_bits == EMELA_UART_STOP_BITS_TWO ? UART_STOP_BITS_2 : UART_STOP_BITS_1,
        .flow_ctrl = UART_HW_FLOWCTRL_DISABLE, .source_clk = UART_SCLK_XTAL};
    esp_err_t error = uart_param_config(UART_NUM_0, &native);
    if (!error) error = uart_set_pin(UART_NUM_0, tx ? EMELA_UART0_TX_PIN : UART_PIN_NO_CHANGE,
                                    rx ? EMELA_UART0_RX_PIN : UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE);
    if (error) {
        for (uint16_t i = 0; i < count; ++i) {
            bool old = false;
            for (uint16_t j = 0; j < claimed_count; ++j) old |= pins[i] == claimed_pins[j];
            if (!old) emelino_idf_pins_release(EMELINO_IDF_OWNER_UART0, &pins[i], 1);
        }
        configured = false;
        return leave(OTHER);
    }
    uart_ll_disable_intr_mask(HW, UINT32_MAX);
    uart_ll_clr_intsts_mask(HW, UINT32_MAX);
    uart_ll_rxfifo_rst(HW); uart_ll_txfifo_rst(HW);
    uart_ll_set_rxfifo_full_thr(HW, 1); uart_ll_set_rx_tout(HW, 2);
    for (uint16_t i = 0; i < claimed_count; ++i) {
        bool keep = false;
        for (uint16_t j = 0; j < count; ++j) keep |= claimed_pins[i] == pins[j];
        if (!keep) {
            gpio_reset_pin(claimed_pins[i]);
            emelino_idf_pins_release(EMELINO_IDF_OWNER_UART0, &claimed_pins[i], 1);
        }
    }
    for (uint16_t i = 0; i < count; ++i) claimed_pins[i] = pins[i];
    claimed_count = count;
    can_transmit = tx; can_receive = rx; notify_transmit = false;
    data_mask = config.data_bits == EMELA_UART_DATA_BITS_BITS7 ? 127 : 255;
    receive_mode = EMELA_UART_RECEIVE_MODE_POLLING;
    read_index = received_count = 0; receive_errors = 0; configured = true;
    return leave(OK);
}
int32_t emela_uart_transmit(EmelaUartId id, const uint16_t *data, uint16_t length, int64_t timeout) {
    if ((!data && length) || !emelino_idf_timeout_valid(timeout)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_transmit) return leave(INVALID_STATE);
    for (uint16_t i = 0; i < length; ++i) if (data[i] > data_mask) return leave(INVALID_ARGUMENT);
    if (!length) return leave(OK);
    uint64_t deadline = emelino_idf_now_us() + (uint64_t)timeout * 1000;
    bool interrupt = interrupt_handle && notify_transmit;
    for (uint16_t i = 0; i < length; ++i) {
        while (!uart_ll_get_txfifo_len(HW)) {
            if (emelino_idf_now_us() >= deadline) return leave(TIMEOUT);
            emelino_idf_poll_wait(deadline);
        }
        if (emelino_idf_now_us() >= deadline) return leave(TIMEOUT);
        uint8_t word = (uint8_t)data[i];
        uint32_t previous = emelino_idf_lock();
        /* Clear stale completion before each write, while the ISR is masked. */
        tx_done = false;
        uart_ll_clr_intsts_mask(HW, UART_INTR_TX_DONE);
        uart_ll_write_txfifo(HW, &word, 1);
        emelino_idf_unlock(previous);
    }
    if (interrupt) uart_ll_ena_intr_mask(HW, UART_INTR_TX_DONE);
    while ((interrupt && !tx_done) || !uart_ll_is_tx_idle(HW)) {
        if (emelino_idf_now_us() >= deadline) { status = TIMEOUT; break; }
        emelino_idf_poll_wait(deadline);
    }
    uart_ll_disable_intr_mask(HW, UART_INTR_TX_DONE);
    if (emelino_idf_now_us() >= deadline) status = TIMEOUT;
    return leave(status);
}
static int32_t take_received(uint16_t *word, bool *available) {
    *available = false;
    uint32_t errors = receive_errors | (uart_ll_get_intraw_mask(HW) & RX_ERRORS);
    receive_errors = 0;
    uart_ll_clr_intsts_mask(HW, RX_ERRORS);
    if (errors) return error_status(errors);
    if (received_count) {
        *word = received[read_index]; read_index = (read_index + 1) % RX_CAPACITY;
        --received_count; *available = true;
    } else if (receive_mode == EMELA_UART_RECEIVE_MODE_POLLING && uart_ll_get_rxfifo_len(HW)) {
        uint8_t byte; uart_ll_read_rxfifo(HW, &byte, 1); *word = byte & data_mask; *available = true;
    }
    return OK;
}
static int32_t receive_words(EmelaUartId id, int32_t size, int64_t timeout,
                            uint16_t *out, uint16_t capacity, uint16_t *length, bool some) {
    if (length) *length = 0;
    if (!out || !length || size <= 0 || !emelino_idf_timeout_valid(timeout)) return INVALID_ARGUMENT;
    if ((uint32_t)size > capacity) return CAPACITY_EXCEEDED;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive || (receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE && !interrupt_handle))
        return leave(INVALID_STATE);
    uint64_t deadline = emelino_idf_now_us() + (uint64_t)timeout * 1000;
    uint16_t count = 0;
    while (count < (uint32_t)size) {
        if (emelino_idf_now_us() >= deadline) { status = TIMEOUT; break; }
        bool available;
        uint32_t previous = emelino_idf_lock();
        status = take_received(out + count, &available);
        emelino_idf_unlock(previous);
        if (status) break;
        if (available) { ++count; continue; }
        if (some && count) break;
        if (emelino_idf_now_us() >= deadline) { status = TIMEOUT; break; }
        emelino_idf_poll_wait(deadline);
    }
    if (!status) *length = count;
    return leave(status);
}
int32_t emela_uart_receive(EmelaUartId id, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout, out, capacity, length, false);
}
int32_t emela_uart_receive_some(EmelaUartId id, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout, out, capacity, length, true);
}
int32_t emela_uart_discard_input(EmelaUartId id, int32_t maximum, int32_t *out) {
    if (!out || maximum < 0) return INVALID_ARGUMENT;
    *out = 0;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(INVALID_STATE);
    uint32_t previous = emelino_idf_lock();
    receive_errors = 0; uart_ll_clr_intsts_mask(HW, RX_ERRORS);
    while (received_count && *out < maximum) {
        read_index = (read_index + 1) % RX_CAPACITY; --received_count; ++*out;
    }
    while (*out < maximum && uart_ll_get_rxfifo_len(HW)) {
        uint8_t word; uart_ll_read_rxfifo(HW, &word, 1); ++*out;
    }
    emelino_idf_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_set_receive_mode(EmelaUartId id, EmelaUartReceiveMode mode) {
    if (mode < EMELA_UART_RECEIVE_MODE_POLLING || mode > EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(INVALID_STATE);
    uint32_t previous = emelino_idf_lock(); receive_mode = mode; receive_interrupts(); emelino_idf_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_enable_interrupt(EmelaUartId id, EmelaInterruptPriority priority, int32_t notify) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH || (notify != 0 && notify != 1))
        return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (notify && !can_transmit) return leave(INVALID_STATE);
    uart_ll_disable_intr_mask(HW, UINT32_MAX);
    if (interrupt_handle) { esp_intr_free(interrupt_handle); interrupt_handle = NULL; }
    int flags = priority == EMELA_INTERRUPT_PRIORITY_LOW ? ESP_INTR_FLAG_LEVEL1
        : priority == EMELA_INTERRUPT_PRIORITY_NORMAL ? ESP_INTR_FLAG_LEVEL2 : ESP_INTR_FLAG_LEVEL3;
    if (esp_intr_alloc(ETS_UART0_INTR_SOURCE, flags, uart_interrupt, NULL, &interrupt_handle) != ESP_OK)
        return leave(RESOURCE_CONFLICT);
    notify_transmit = notify != 0;
    receive_interrupts();
    return leave(OK);
}
int32_t emela_uart_disable_interrupt(EmelaUartId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uart_ll_disable_intr_mask(HW, UINT32_MAX);
    if (interrupt_handle) { esp_intr_free(interrupt_handle); interrupt_handle = NULL; }
    notify_transmit = false;
    return leave(OK);
}
