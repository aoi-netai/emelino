/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"
#include "emela_uart.h"

/* Legacy UART0 deliberately avoids a DMA buffer and shares no controller with
   SPI2/TWI0. The API remains synchronous even when IRQ reception is selected. */
enum { OK, INVALID_ARGUMENT, CAPACITY_EXCEEDED, NOT_CONFIGURED, UNSUPPORTED,
       TIMEOUT, BUSY, RESOURCE_CONFLICT, INVALID_STATE, PARITY, FRAMING, OVERRUN, OTHER };
#define RX_CAPACITY 64u
#define RX_INTERRUPTS (UART_INTENSET_RXDRDY_Msk | UART_INTENSET_ERROR_Msk)
#define UART_INTERRUPTS (RX_INTERRUPTS | UART_INTENSET_TXDRDY_Msk)
#define UART_ERRORS (UART_ERRORSRC_OVERRUN_Msk | UART_ERRORSRC_PARITY_Msk | \
                     UART_ERRORSRC_FRAMING_Msk | UART_ERRORSRC_BREAK_Msk)
static bool configured, can_receive, can_transmit, irq_enabled, notify_transmit;
static bool occupied;
static EmelaUartReceiveMode receive_mode;
static volatile bool tx_done;
static uint16_t received[RX_CAPACITY];
static volatile uint16_t read_index, write_index, received_count;
static volatile uint32_t receive_errors;
static int32_t claimed_pins[2];
static uint16_t claimed_count;

static NRF_GPIO_Type *port(int32_t pin) { return pin < 32 ? NRF_P0 : NRF_P1; }
static uint32_t bit(int32_t pin) { return UINT32_C(1) << ((uint32_t)pin % 32); }
static bool valid_timeout(int64_t timeout) { return timeout > 0 && (uint64_t)timeout <= UINT32_MAX; }
static int32_t valid_id(EmelaUartId id) {
    if (id < EMELA_UART_ID_UART0 || id > EMELA_UART_ID_UART7) return INVALID_ARGUMENT;
    return id == EMELA_UART_ID_UART0 ? OK : UNSUPPORTED;
}
static int32_t enter(EmelaUartId id, bool require_configured) {
    int32_t result = valid_id(id);
    if (result) return result;
    if (nrf52_exception_number() || __get_PRIMASK()) return INVALID_STATE;
    uint32_t previous = nrf52_interrupt_lock();
    if (require_configured && !configured) result = NOT_CONFIGURED;
    else if (occupied) result = BUSY;
    else occupied = true;
    nrf52_interrupt_unlock(previous);
    return result;
}
static int32_t leave(int32_t result) {
    uint32_t previous = nrf52_interrupt_lock();
    occupied = false;
    nrf52_interrupt_unlock(previous);
    return result;
}
static int32_t error_status(uint32_t errors) {
    if (errors & UART_ERRORSRC_OVERRUN_Msk) return OVERRUN;
    if (errors & UART_ERRORSRC_PARITY_Msk) return PARITY;
    if (errors & UART_ERRORSRC_FRAMING_Msk) return FRAMING;
    return errors ? OTHER : OK;
}
static void receive_interrupts(void) {
    NRF_UART0->INTENCLR = RX_INTERRUPTS;
    if (irq_enabled && can_receive && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        NRF_UART0->INTENSET = RX_INTERRUPTS;
}

/* Nordic's documented settings, using the closest nominal rate within 3%.
   Reject an unrepresentable request instead of silently changing its meaning. */
static bool baud_rate(int32_t requested, uint32_t *setting) {
    static const struct { uint32_t rate, value; } rates[] = {
        {1200, UART_BAUDRATE_BAUDRATE_Baud1200}, {2400, UART_BAUDRATE_BAUDRATE_Baud2400},
        {4800, UART_BAUDRATE_BAUDRATE_Baud4800}, {9600, UART_BAUDRATE_BAUDRATE_Baud9600},
        {14400, UART_BAUDRATE_BAUDRATE_Baud14400}, {19200, UART_BAUDRATE_BAUDRATE_Baud19200},
        {28800, UART_BAUDRATE_BAUDRATE_Baud28800}, {31250, UART_BAUDRATE_BAUDRATE_Baud31250},
        {38400, UART_BAUDRATE_BAUDRATE_Baud38400}, {56000, UART_BAUDRATE_BAUDRATE_Baud56000},
        {57600, UART_BAUDRATE_BAUDRATE_Baud57600}, {76800, UART_BAUDRATE_BAUDRATE_Baud76800},
        {115200, UART_BAUDRATE_BAUDRATE_Baud115200}, {230400, UART_BAUDRATE_BAUDRATE_Baud230400},
        {250000, UART_BAUDRATE_BAUDRATE_Baud250000}, {460800, UART_BAUDRATE_BAUDRATE_Baud460800},
        {921600, UART_BAUDRATE_BAUDRATE_Baud921600}, {1000000, UART_BAUDRATE_BAUDRATE_Baud1M},
    };
    uint32_t best = UINT32_MAX;
    for (uint32_t i = 0; i < sizeof(rates) / sizeof(rates[0]); ++i) {
        uint32_t rate = rates[i].rate, want = (uint32_t)requested;
        uint32_t distance = rate > want ? rate - want : want - rate;
        if (distance < best) { best = distance; *setting = rates[i].value; }
    }
    return (uint64_t)best * 100 <= (uint64_t)(uint32_t)requested * 3;
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
    if (config.data_bits != EMELA_UART_DATA_BITS_BITS8 || config.parity == EMELA_UART_PARITY_ODD
        || (config.stop_bits != EMELA_UART_STOP_BITS_ONE && config.stop_bits != EMELA_UART_STOP_BITS_TWO)
        || config.flow_control != EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.oversampling != EMELA_UART_OVERSAMPLING_NATIVE) return UNSUPPORTED;
    uint32_t baud = 0;
    if (!baud_rate(config.baud_rate, &baud)) return UNSUPPORTED;
    bool rx = config.direction != EMELA_UART_DIRECTION_TRANSMIT;
    bool tx = config.direction != EMELA_UART_DIRECTION_RECEIVE;
    int32_t pins[2];
    uint16_t count = 0;
    if (tx) pins[count++] = EMELA_UART0_TX_PIN;
    if (rx) pins[count++] = EMELA_UART0_RX_PIN;
    status = enter(id, false);
    if (status) return status;
    int32_t claim = nrf52_pins_claim(NRF52_OWNER_UART0, pins, count);
    if (claim) return leave(claim == 5 ? RESOURCE_CONFLICT : INVALID_ARGUMENT);
    uint32_t previous = nrf52_interrupt_lock();
    NVIC_DisableIRQ(UARTE0_UART0_IRQn);
    NRF_UART0->INTENCLR = UART_INTERRUPTS;
    if (configured) {
        NRF_UART0->TASKS_STOPRX = 1;
        NRF_UART0->TASKS_STOPTX = 1;
    }
    NRF_UART0->ENABLE = 0;
    if (tx) {
        port(EMELA_UART0_TX_PIN)->OUTSET = bit(EMELA_UART0_TX_PIN);
        port(EMELA_UART0_TX_PIN)->PIN_CNF[EMELA_UART0_TX_PIN % 32] =
            (GPIO_PIN_CNF_DIR_Output << GPIO_PIN_CNF_DIR_Pos) |
            (GPIO_PIN_CNF_INPUT_Disconnect << GPIO_PIN_CNF_INPUT_Pos);
    }
    if (rx) port(EMELA_UART0_RX_PIN)->PIN_CNF[EMELA_UART0_RX_PIN % 32] =
        GPIO_PIN_CNF_PULL_Pullup << GPIO_PIN_CNF_PULL_Pos;
    NRF_UART0->PSEL.TXD = tx ? EMELA_UART0_TX_PIN : UINT32_MAX;
    NRF_UART0->PSEL.RXD = rx ? EMELA_UART0_RX_PIN : UINT32_MAX;
    NRF_UART0->PSEL.RTS = UINT32_MAX;
    NRF_UART0->PSEL.CTS = UINT32_MAX;
    NRF_UART0->BAUDRATE = baud;
    NRF_UART0->CONFIG =
        (config.parity == EMELA_UART_PARITY_EVEN ? UART_CONFIG_PARITY_Included << UART_CONFIG_PARITY_Pos : 0)
        | (config.stop_bits == EMELA_UART_STOP_BITS_TWO ? UART_CONFIG_STOP_Two << UART_CONFIG_STOP_Pos : 0);
    NRF_UART0->EVENTS_RXDRDY = NRF_UART0->EVENTS_TXDRDY = NRF_UART0->EVENTS_ERROR = 0;
    NRF_UART0->ERRORSRC = UART_ERRORS;
    NRF_UART0->ENABLE = UART_ENABLE_ENABLE_Enabled;
    if (rx) NRF_UART0->TASKS_STARTRX = 1;
    can_receive = rx;
    can_transmit = tx;
    configured = true;
    irq_enabled = notify_transmit = tx_done = false;
    receive_mode = EMELA_UART_RECEIVE_MODE_POLLING;
    read_index = write_index = received_count = 0;
    receive_errors = 0;
    NVIC_ClearPendingIRQ(UARTE0_UART0_IRQn);
    for (uint16_t old = 0; old < claimed_count; ++old) {
        bool keep = false;
        for (uint16_t i = 0; i < count; ++i) keep |= claimed_pins[old] == pins[i];
        if (!keep) {
            port(claimed_pins[old])->PIN_CNF[claimed_pins[old] % 32] =
                GPIO_PIN_CNF_INPUT_Disconnect << GPIO_PIN_CNF_INPUT_Pos;
            nrf52_pins_release(NRF52_OWNER_UART0, &claimed_pins[old], 1);
        }
    }
    for (uint16_t i = 0; i < count; ++i) claimed_pins[i] = pins[i];
    claimed_count = count;
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}

int32_t emela_uart_transmit(EmelaUartId id, const uint16_t *data, uint16_t length, int64_t timeout) {
    if (!valid_timeout(timeout) || (!data && length)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_transmit) return leave(INVALID_STATE);
    for (uint16_t i = 0; i < length; ++i) if (data[i] > 255) return leave(INVALID_ARGUMENT);
    if (!length) return leave(OK);
    uint64_t deadline = nrf52_now_us() + (uint64_t)timeout * 1000;
    NRF_UART0->TASKS_STARTTX = 1;
    for (uint16_t i = 0; i < length; ++i) {
        if (nrf52_now_us() >= deadline) { status = TIMEOUT; break; }
        uint32_t previous = nrf52_interrupt_lock();
        NRF_UART0->INTENCLR = UART_INTENSET_TXDRDY_Msk;
        NRF_UART0->EVENTS_TXDRDY = 0;
        tx_done = false;
        bool interrupt = irq_enabled && notify_transmit;
        if (interrupt) NRF_UART0->INTENSET = UART_INTENSET_TXDRDY_Msk;
        NRF_UART0->TXD = data[i];
        nrf52_interrupt_unlock(previous);
        while (interrupt ? !tx_done : !NRF_UART0->EVENTS_TXDRDY) {
            if (nrf52_now_us() >= deadline) { status = TIMEOUT; break; }
        }
        if (nrf52_now_us() >= deadline) status = TIMEOUT;
        if (status) break;
    }
    NRF_UART0->INTENCLR = UART_INTENSET_TXDRDY_Msk;
    NRF_UART0->TASKS_STOPTX = 1;
    return leave(status);
}

/* Call with interrupts masked. Clearing RXDRDY before RXD preserves the next
   hardware event, including when the receiver FIFO already contains more data. */
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
    if (NRF_UART0->EVENTS_ERROR) {
        uint32_t errors = NRF_UART0->ERRORSRC;
        NRF_UART0->ERRORSRC = errors;
        NRF_UART0->EVENTS_ERROR = 0;
        return errors ? error_status(errors) : OTHER;
    }
    if (NRF_UART0->EVENTS_RXDRDY) {
        NRF_UART0->EVENTS_RXDRDY = 0;
        *word = (uint16_t)NRF_UART0->RXD;
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
    uint64_t deadline = nrf52_now_us() + (uint64_t)timeout * 1000;
    uint16_t count = 0;
    while (count < (uint32_t)size) {
        if (nrf52_now_us() >= deadline) return leave(TIMEOUT);
        bool available;
        uint32_t previous = nrf52_interrupt_lock();
        status = take_received(&out[count], &available);
        nrf52_interrupt_unlock(previous);
        if (status) return leave(status);
        if (available) count++;
        else if (some && count) break;
    }
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
    uint32_t previous = nrf52_interrupt_lock();
    receive_errors = 0;
    NRF_UART0->ERRORSRC = NRF_UART0->ERRORSRC;
    NRF_UART0->EVENTS_ERROR = 0;
    while (received_count && count < max_size) {
        read_index = (uint16_t)((read_index + 1) % RX_CAPACITY);
        received_count--;
        ++count;
    }
    /* Hardware has a six-byte RX FIFO. Never wait for additional input. */
    for (unsigned i = 0; i < 6 && count < max_size && NRF_UART0->EVENTS_RXDRDY; ++i) {
        NRF_UART0->EVENTS_RXDRDY = 0;
        (void)NRF_UART0->RXD;
        ++count;
    }
    nrf52_interrupt_unlock(previous);
    *out = count;
    return leave(OK);
}
int32_t emela_uart_set_receive_mode(EmelaUartId id, EmelaUartReceiveMode mode) {
    if (mode < EMELA_UART_RECEIVE_MODE_POLLING || mode > EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(INVALID_STATE);
    uint32_t previous = nrf52_interrupt_lock();
    receive_mode = mode;
    receive_interrupts();
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_enable_interrupt(EmelaUartId id, EmelaInterruptPriority priority, int32_t notify) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH
        || (notify != 0 && notify != 1)) return INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (notify && !can_transmit) return leave(INVALID_STATE);
    uint32_t previous = nrf52_interrupt_lock();
    irq_enabled = true;
    notify_transmit = notify != 0;
    NVIC_SetPriority(UARTE0_UART0_IRQn, 6 - (uint32_t)priority * 2);
    NVIC_ClearPendingIRQ(UARTE0_UART0_IRQn);
    receive_interrupts();
    NVIC_EnableIRQ(UARTE0_UART0_IRQn);
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}
int32_t emela_uart_disable_interrupt(EmelaUartId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = nrf52_interrupt_lock();
    irq_enabled = notify_transmit = false;
    NRF_UART0->INTENCLR = UART_INTERRUPTS;
    NVIC_DisableIRQ(UARTE0_UART0_IRQn);
    NVIC_ClearPendingIRQ(UARTE0_UART0_IRQn);
    nrf52_interrupt_unlock(previous);
    return leave(OK);
}
void UARTE0_UART0_IRQHandler(void) {
    if (NRF_UART0->EVENTS_ERROR) {
        uint32_t errors = NRF_UART0->ERRORSRC;
        NRF_UART0->ERRORSRC = errors;
        NRF_UART0->EVENTS_ERROR = 0;
        receive_errors |= errors ? errors : UART_ERRORSRC_BREAK_Msk;
    }
    if (irq_enabled && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE && NRF_UART0->EVENTS_RXDRDY) {
        NRF_UART0->EVENTS_RXDRDY = 0;
        uint16_t word = (uint16_t)NRF_UART0->RXD;
        if (received_count == RX_CAPACITY) receive_errors |= UART_ERRORSRC_OVERRUN_Msk;
        else {
            received[write_index] = word;
            write_index = (uint16_t)((write_index + 1) % RX_CAPACITY);
            ++received_count;
        }
    }
    if ((NRF_UART0->INTENSET & UART_INTENSET_TXDRDY_Msk) && NRF_UART0->EVENTS_TXDRDY) {
        NRF_UART0->EVENTS_TXDRDY = 0;
        NRF_UART0->INTENCLR = UART_INTENSET_TXDRDY_Msk;
        tx_done = true;
    }
    /* Nordic peripheral interrupt clears can take up to four CPU cycles. */
    (void)NRF_UART0->INTENCLR;
    __DSB();
}
