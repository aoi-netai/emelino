/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emela_uart.h"
#include "hardware/clocks.h"
#include "hardware/gpio.h"
#include "hardware/irq.h"
#include "hardware/sync.h"
#include "hardware/uart.h"

enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, PARITY, FRAMING, OVERRUN, OTHER };
#define RX_CAPACITY 64u
#define RX_INTERRUPTS (UART_UARTIMSC_RXIM_BITS | UART_UARTIMSC_RTIM_BITS | \
    UART_UARTIMSC_OEIM_BITS | UART_UARTIMSC_BEIM_BITS | UART_UARTIMSC_PEIM_BITS | UART_UARTIMSC_FEIM_BITS)
#define RX_ERRORS (UART_UARTDR_OE_BITS | UART_UARTDR_BE_BITS | UART_UARTDR_PE_BITS | UART_UARTDR_FE_BITS)
static bool configured, can_receive, can_transmit, irq_enabled, notify_transmit, occupied;
static EmelaUartReceiveMode receive_mode;
static uint16_t data_mask;
static volatile bool tx_done;
static uint16_t received[RX_CAPACITY];
static volatile uint16_t read_index, write_index, received_count;
static volatile uint32_t receive_errors;
static int32_t claimed_pins[2];
static uint16_t claimed_count;

static bool valid_timeout(int64_t timeout) { return timeout > 0 && (uint64_t)timeout <= UINT32_MAX; }
static int32_t valid_id(EmelaUartId id) {
    if (id < EMELA_UART_ID_UART0 || id > EMELA_UART_ID_UART7) return INVALID_ARGUMENT;
    return id == EMELA_UART_ID_UART0 ? OK : UNSUPPORTED;
}
static int32_t enter(EmelaUartId id, bool require_configured) {
    int32_t result = valid_id(id);
    if (result) return result;
    if (get_core_num() || emelino_pico_exception_number()) return INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (previous & 1u) result = INVALID_STATE;
    else if (require_configured && !configured) result = NOT_CONFIGURED;
    else if (occupied) result = BUSY;
    else occupied = true;
    emelino_pico_interrupt_unlock(previous);
    return result;
}
static int32_t leave(int32_t result) {
    uint32_t previous = emelino_pico_interrupt_lock();
    occupied = false;
    emelino_pico_interrupt_unlock(previous);
    return result;
}
static int32_t error_status(uint32_t errors) {
    if (errors & UART_UARTDR_OE_BITS) return OVERRUN;
    if (errors & UART_UARTDR_PE_BITS) return PARITY;
    if (errors & UART_UARTDR_FE_BITS) return FRAMING;
    return errors ? OTHER : OK;
}
static void receive_interrupts(void) {
    hw_clear_bits(&uart_get_hw(uart0)->imsc, RX_INTERRUPTS);
    if (irq_enabled && can_receive && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        hw_set_bits(&uart_get_hw(uart0)->imsc, RX_INTERRUPTS);
}
static bool baud_supported(int32_t requested) {
    uint64_t clock = clock_get_hz(UART_CLOCK_NUM(uart0));
    if (!clock) return false;
    uint64_t divisor = (8 * clock / (uint32_t)requested + 1) / 2;
    if (divisor < 64 || divisor >= 65535 * 64) return false;
    uint64_t actual = 4 * clock / divisor;
    uint64_t error = actual > (uint32_t)requested ? actual - (uint32_t)requested : (uint32_t)requested - actual;
    return error * 100 <= (uint64_t)(uint32_t)requested * 3;
}

static void uart_interrupt(void) {
    uart_hw_t *hw = uart_get_hw(uart0);
    if (irq_enabled && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) {
        /* Bound ISR work even while input arrives continuously. */
        for (unsigned i = 0; i < 32 && uart_is_readable(uart0); ++i) {
            uint32_t value = hw->dr;
            receive_errors |= value & RX_ERRORS;
            if (received_count == RX_CAPACITY) receive_errors |= UART_UARTDR_OE_BITS;
            else {
                received[write_index] = (uint16_t)value & data_mask;
                write_index = (uint16_t)((write_index + 1) % RX_CAPACITY);
                ++received_count;
            }
        }
        /* RSR uses bits 0..3, DR the corresponding bits 8..11. */
        receive_errors |= (hw->rsr << 8) & RX_ERRORS;
        hw->rsr = 0;
        hw->icr = RX_INTERRUPTS;
    }
    if (hw->mis & UART_UARTMIS_TXMIS_BITS) {
        hw_clear_bits(&hw->imsc, UART_UARTIMSC_TXIM_BITS);
        hw->icr = UART_UARTICR_TXIC_BITS;
        tx_done = true;
    }
}

int32_t emela_uart_configure(EmelaUartId id, EmelaUartConfig config) {
    int32_t status = valid_id(id);
    if (status) return status;
    if (config.baud_rate <= 0 || config.data_bits < EMELA_UART_DATA_BITS_BITS7
        || config.data_bits > EMELA_UART_DATA_BITS_BITS9 || config.stop_bits < EMELA_UART_STOP_BITS_HALF
        || config.stop_bits > EMELA_UART_STOP_BITS_TWO || config.parity < EMELA_UART_PARITY_NO_PARITY
        || config.parity > EMELA_UART_PARITY_ODD || config.direction < EMELA_UART_DIRECTION_RECEIVE
        || config.direction > EMELA_UART_DIRECTION_BOTH || config.flow_control < EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.flow_control > EMELA_UART_FLOW_CONTROL_RTS_CTS || config.oversampling < EMELA_UART_OVERSAMPLING_BY8
        || config.oversampling > EMELA_UART_OVERSAMPLING_NATIVE) return INVALID_ARGUMENT;
    if (config.data_bits == EMELA_UART_DATA_BITS_BITS9
        || (config.stop_bits != EMELA_UART_STOP_BITS_ONE && config.stop_bits != EMELA_UART_STOP_BITS_TWO)
        || config.flow_control != EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.oversampling != EMELA_UART_OVERSAMPLING_NATIVE || !baud_supported(config.baud_rate)) return UNSUPPORTED;
    bool rx = config.direction != EMELA_UART_DIRECTION_TRANSMIT;
    bool tx = config.direction != EMELA_UART_DIRECTION_RECEIVE;
    int32_t pins[2];
    uint16_t count = 0;
    if (tx) pins[count++] = EMELA_UART0_TX_PIN;
    if (rx) pins[count++] = EMELA_UART0_RX_PIN;
    status = enter(id, false);
    if (status) return status;
    int32_t claim = emelino_pico_pins_claim(EMELINO_PICO_OWNER_UART0, pins, count);
    if (claim) return leave(claim == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
    uint32_t previous = emelino_pico_interrupt_lock();
    irq_set_enabled(UART0_IRQ, false);
    uart_init(uart0, (uint32_t)config.baud_rate);
    uart_hw_t *hw = uart_get_hw(uart0);
    hw->cr = 0;
    hw->imsc = hw->dmacr = 0;
    /* A single hardware holding register makes TX-empty IRQs unambiguous even
       for a one-word transfer. Interrupt RX still has a 64-word software FIFO. */
    uart_set_fifo_enabled(uart0, false);
    uart_set_format(uart0, config.data_bits == EMELA_UART_DATA_BITS_BITS7 ? 7 : 8,
        config.stop_bits == EMELA_UART_STOP_BITS_TWO ? 2 : 1,
        config.parity == EMELA_UART_PARITY_EVEN ? UART_PARITY_EVEN
            : config.parity == EMELA_UART_PARITY_ODD ? UART_PARITY_ODD : UART_PARITY_NONE);
    if (tx) {
        gpio_disable_pulls(EMELA_UART0_TX_PIN);
        gpio_set_function(EMELA_UART0_TX_PIN, UART_FUNCSEL_NUM(uart0, EMELA_UART0_TX_PIN));
    }
    if (rx) {
        gpio_pull_up(EMELA_UART0_RX_PIN);
        gpio_set_function(EMELA_UART0_RX_PIN, UART_FUNCSEL_NUM(uart0, EMELA_UART0_RX_PIN));
    }
    hw->rsr = 0;
    hw->icr = UART_UARTICR_BITS;
    hw->cr = UART_UARTCR_UARTEN_BITS | (tx ? UART_UARTCR_TXE_BITS : 0) | (rx ? UART_UARTCR_RXE_BITS : 0);
    can_receive = rx;
    can_transmit = tx;
    data_mask = config.data_bits == EMELA_UART_DATA_BITS_BITS7 ? 127 : 255;
    configured = true;
    irq_enabled = notify_transmit = tx_done = false;
    receive_mode = EMELA_UART_RECEIVE_MODE_POLLING;
    read_index = write_index = received_count = 0;
    receive_errors = 0;
    irq_clear(UART0_IRQ);
    for (uint16_t old = 0; old < claimed_count; ++old) {
        bool keep = false;
        for (uint16_t i = 0; i < count; ++i) keep |= claimed_pins[old] == pins[i];
        if (!keep) {
            gpio_set_function(claimed_pins[old], GPIO_FUNC_NULL);
            gpio_disable_pulls(claimed_pins[old]);
            emelino_pico_pins_release(EMELINO_PICO_OWNER_UART0, &claimed_pins[old], 1);
        }
    }
    for (uint16_t i = 0; i < count; ++i) claimed_pins[i] = pins[i];
    claimed_count = count;
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}

int32_t emela_uart_transmit(EmelaUartId id, const uint16_t *data, uint16_t length, int64_t timeout) {
    if (!valid_timeout(timeout) || (!data && length)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_transmit) return leave(INVALID_STATE);
    for (uint16_t i = 0; i < length; ++i) if (data[i] > data_mask) return leave(INVALID_ARGUMENT);
    if (!length) return leave(OK);
    uint64_t deadline = emelino_pico_now_us() + (uint64_t)timeout * 1000;
    bool interrupt = irq_enabled && notify_transmit;
    for (uint16_t i = 0; i < length && !status; ++i) {
        while (!uart_is_writable(uart0)) {
            if (emelino_pico_now_us() >= deadline) { status = TIMEOUT; break; }
        }
        if (emelino_pico_now_us() >= deadline) status = TIMEOUT;
        if (status) break;
        uint32_t previous = emelino_pico_interrupt_lock();
        tx_done = false;
        uart_get_hw(uart0)->icr = UART_UARTICR_TXIC_BITS;
        uart_get_hw(uart0)->dr = data[i];
        if (interrupt) hw_set_bits(&uart_get_hw(uart0)->imsc, UART_UARTIMSC_TXIM_BITS);
        emelino_pico_interrupt_unlock(previous);
        while (interrupt && !tx_done) {
            if (emelino_pico_now_us() >= deadline) { status = TIMEOUT; break; }
        }
    }
    /* TX-empty only covers the holding register. Wait for the shift register
       and last stop bit as well, within the same operation-wide deadline. */
    while (!status && (uart_get_hw(uart0)->fr & UART_UARTFR_BUSY_BITS)) {
        if (emelino_pico_now_us() >= deadline) status = TIMEOUT;
    }
    if (!status && emelino_pico_now_us() >= deadline) status = TIMEOUT;
    hw_clear_bits(&uart_get_hw(uart0)->imsc, UART_UARTIMSC_TXIM_BITS);
    return leave(status);
}

/* Called with IRQs masked; buffered words survive a switch back to polling. */
static int32_t take_received(uint16_t *word, bool *available) {
    *available = false;
    if (receive_errors) {
        int32_t result = error_status(receive_errors);
        receive_errors = 0;
        return result;
    }
    if (received_count) {
        *word = received[read_index];
        read_index = (uint16_t)((read_index + 1) % RX_CAPACITY);
        received_count--;
        *available = true;
        return OK;
    }
    if (receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) return OK;
    if (uart_is_readable(uart0)) {
        uint32_t value = uart_get_hw(uart0)->dr;
        uart_get_hw(uart0)->rsr = 0;
        if (value & RX_ERRORS) return error_status(value);
        *word = (uint16_t)value & data_mask;
        *available = true;
    }
    return OK;
}
static int32_t receive_words(EmelaUartId id, int32_t size, int64_t timeout,
                            uint16_t *out, uint16_t capacity, uint16_t *length, bool some) {
    if (length) *length = 0;
    if (size <= 0 || !out || !length || !valid_timeout(timeout)) return INVALID_ARGUMENT;
    if ((uint32_t)size > capacity) return CAPACITY_EXCEEDED;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive || (receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE && !irq_enabled))
        return leave(INVALID_STATE);
    uint64_t deadline = emelino_pico_now_us() + (uint64_t)timeout * 1000;
    uint16_t count = 0;
    while (count < (uint32_t)size) {
        if (emelino_pico_now_us() >= deadline) return leave(TIMEOUT);
        bool available;
        uint32_t previous = emelino_pico_interrupt_lock();
        status = take_received(&out[count], &available);
        emelino_pico_interrupt_unlock(previous);
        if (status) return leave(status);
        if (available) count++;
        else if (some && count) break;
    }
    if (emelino_pico_now_us() >= deadline) return leave(TIMEOUT);
    *length = count;
    return leave(OK);
}
int32_t emela_uart_receive(EmelaUartId id, int32_t size, int64_t timeout,
                          uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout, out, capacity, length, false);
}
int32_t emela_uart_receive_some(EmelaUartId id, int32_t size, int64_t timeout,
                               uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout, out, capacity, length, true);
}
int32_t emela_uart_discard_input(EmelaUartId id, int32_t max_size, int32_t *out) {
    if (max_size <= 0 || !out) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(INVALID_STATE);
    int32_t count = 0;
    uint32_t previous = emelino_pico_interrupt_lock();
    receive_errors = 0;
    uart_get_hw(uart0)->rsr = 0;
    uart_get_hw(uart0)->icr = RX_INTERRUPTS;
    while (received_count && count < max_size) {
        read_index = (uint16_t)((read_index + 1) % RX_CAPACITY);
        received_count--;
        ++count;
    }
    /* FIFO is disabled: discard the holding register without waiting. */
    if (count < max_size && uart_is_readable(uart0)) {
        (void)uart_get_hw(uart0)->dr;
        ++count;
    }
    emelino_pico_interrupt_unlock(previous);
    *out = count;
    return leave(OK);
}
int32_t emela_uart_set_receive_mode(EmelaUartId id, EmelaUartReceiveMode mode) {
    if (mode < EMELA_UART_RECEIVE_MODE_POLLING || mode > EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(INVALID_STATE);
    uint32_t previous = emelino_pico_interrupt_lock();
    receive_mode = mode;
    receive_interrupts();
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_enable_interrupt(EmelaUartId id, EmelaInterruptPriority priority, int32_t notify) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH
        || (notify != 0 && notify != 1)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (notify && !can_transmit) return leave(INVALID_STATE);
    uint32_t previous = emelino_pico_interrupt_lock();
    irq_enabled = true;
    notify_transmit = notify != 0;
    irq_set_exclusive_handler(UART0_IRQ, uart_interrupt);
    irq_set_priority(UART0_IRQ, 0xc0u - (uint32_t)priority * 0x40u);
    irq_clear(UART0_IRQ);
    receive_interrupts();
    irq_set_enabled(UART0_IRQ, true);
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_disable_interrupt(EmelaUartId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = emelino_pico_interrupt_lock();
    irq_enabled = notify_transmit = false;
    uart_get_hw(uart0)->imsc = 0;
    irq_set_enabled(UART0_IRQ, false);
    irq_clear(UART0_IRQ);
    emelino_pico_interrupt_unlock(previous);
    return leave(OK);
}
