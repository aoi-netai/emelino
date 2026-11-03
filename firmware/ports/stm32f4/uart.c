/* SPDX-License-Identifier: Apache-2.0 */
#include <stdbool.h>
#include <stdint.h>

#include "board_config.h"
#include "bsp.h"
#include "emela_uart.h"
#include "registers.h"

/* UartErrorKind ordinal + 1. Zero is success. */
enum {
    UART_OK = 0, UART_INVALID_ARGUMENT = 1, UART_CAPACITY_EXCEEDED = 2,
    UART_NOT_CONFIGURED = 3, UART_UNSUPPORTED = 4, UART_TIMEOUT = 5,
    UART_BUSY = 6, UART_RESOURCE_CONFLICT = 7, UART_INVALID_STATE = 8,
    UART_PARITY = 9, UART_FRAMING = 10, UART_OVERRUN = 11, UART_OTHER = 12,
};
#define RX_CAPACITY 64u
#define RX_ERRORS (USART_SR_PE | USART_SR_FE | USART_SR_NE | USART_SR_ORE)

static UART_HandleTypeDef uart;
static bool configured;
static bool can_receive;
static bool can_transmit;
static uint16_t data_mask;
static volatile bool busy;
static volatile bool irq_enabled;
static volatile bool notify_transmit;
static volatile bool transmit_complete;
static volatile EmelaUartReceiveMode receive_mode;
static uint16_t received[RX_CAPACITY];
static volatile uint16_t read_index;
static volatile uint16_t write_index;
static volatile uint16_t received_count;
static volatile uint32_t receive_errors;
static int32_t claimed_pins[2];
static uint16_t claimed_count;

static GPIO_TypeDef *const ports[] = {GPIOA, GPIOB, GPIOC, GPIOD, GPIOE, GPIOF, GPIOG, GPIOH};

static void enable_gpio_clock(uint32_t port) {
    switch (port) {
    case 0: __HAL_RCC_GPIOA_CLK_ENABLE(); break;
    case 1: __HAL_RCC_GPIOB_CLK_ENABLE(); break;
    case 2: __HAL_RCC_GPIOC_CLK_ENABLE(); break;
    case 3: __HAL_RCC_GPIOD_CLK_ENABLE(); break;
    case 4: __HAL_RCC_GPIOE_CLK_ENABLE(); break;
    case 5: __HAL_RCC_GPIOF_CLK_ENABLE(); break;
    case 6: __HAL_RCC_GPIOG_CLK_ENABLE(); break;
    case 7: __HAL_RCC_GPIOH_CLK_ENABLE(); break;
    }
}

static int32_t validate_id(EmelaUartId id) {
    if (id < EMELA_UART_ID_UART0 || id > EMELA_UART_ID_UART7) return UART_INVALID_ARGUMENT;
    return id == EMELA_UART_ID_UART2 ? UART_OK : UART_UNSUPPORTED;
}

static int32_t enter(EmelaUartId id, bool require_configured) {
    int32_t status = validate_id(id);
    if (status) return status;
    if (stm32f4_exception_number() || __get_PRIMASK()) return UART_INVALID_STATE;
    uint32_t previous = stm32f4_interrupt_lock();
    if (require_configured && !configured) status = UART_NOT_CONFIGURED;
    else if (busy) status = UART_BUSY;
    else busy = true;
    stm32f4_interrupt_unlock(previous);
    return status;
}

static int32_t leave(int32_t status) {
    uint32_t previous = stm32f4_interrupt_lock();
    busy = false;
    stm32f4_interrupt_unlock(previous);
    return status;
}

static bool valid_timeout(int64_t timeout_ms) {
    return timeout_ms > 0 && (uint64_t)timeout_ms <= UINT32_MAX;
}

static bool expired(uint32_t start, uint32_t timeout) {
    return (uint32_t)(HAL_GetTick() - start) >= timeout;
}

static int32_t error_status(uint32_t errors) {
    if (errors & USART_SR_ORE) return UART_OVERRUN;
    if (errors & USART_SR_PE) return UART_PARITY;
    if (errors & USART_SR_FE) return UART_FRAMING;
    return errors ? UART_OTHER : UART_OK;
}

static void update_rx_interrupt(void) {
    if (irq_enabled && can_receive && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) {
        __HAL_UART_ENABLE_IT(&uart, UART_IT_PE);
        __HAL_UART_ENABLE_IT(&uart, UART_IT_ERR);
        __HAL_UART_ENABLE_IT(&uart, UART_IT_RXNE);
    } else {
        __HAL_UART_DISABLE_IT(&uart, UART_IT_RXNE);
        __HAL_UART_DISABLE_IT(&uart, UART_IT_PE);
        __HAL_UART_DISABLE_IT(&uart, UART_IT_ERR);
    }
}

int32_t emela_uart_configure(EmelaUartId id, EmelaUartConfig config) {
    int32_t status = validate_id(id);
    if (status) return status;
    if (config.baud_rate <= 0 || config.data_bits < EMELA_UART_DATA_BITS_BITS7
        || config.data_bits > EMELA_UART_DATA_BITS_BITS9 || config.stop_bits < EMELA_UART_STOP_BITS_HALF
        || config.stop_bits > EMELA_UART_STOP_BITS_TWO || config.parity < EMELA_UART_PARITY_NO_PARITY
        || config.parity > EMELA_UART_PARITY_ODD || config.direction < EMELA_UART_DIRECTION_RECEIVE
        || config.direction > EMELA_UART_DIRECTION_BOTH || config.flow_control < EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL
        || config.flow_control > EMELA_UART_FLOW_CONTROL_RTS_CTS || config.oversampling < EMELA_UART_OVERSAMPLING_BY8
        || config.oversampling > EMELA_UART_OVERSAMPLING_NATIVE) return UART_INVALID_ARGUMENT;
    if (config.flow_control != EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL) return UART_UNSUPPORTED;
    if (config.stop_bits != EMELA_UART_STOP_BITS_ONE && config.stop_bits != EMELA_UART_STOP_BITS_TWO)
        return UART_UNSUPPORTED;
    uint32_t bits = 7u + (uint32_t)config.data_bits;
    uint32_t parity = config.parity == EMELA_UART_PARITY_NO_PARITY ? UART_PARITY_NONE
        : config.parity == EMELA_UART_PARITY_EVEN ? UART_PARITY_EVEN : UART_PARITY_ODD;
    uint32_t hardware_bits = bits + (parity != UART_PARITY_NONE);
    if (hardware_bits < 8 || hardware_bits > 9) return UART_UNSUPPORTED;
    uint32_t clock = HAL_RCC_GetPCLK1Freq();
    uint32_t oversampling = config.oversampling == EMELA_UART_OVERSAMPLING_BY8 ? UART_OVERSAMPLING_8 : UART_OVERSAMPLING_16;
    if (config.oversampling == EMELA_UART_OVERSAMPLING_NATIVE && (uint32_t)config.baud_rate > clock / 16)
        oversampling = UART_OVERSAMPLING_8;
    uint32_t divisor = (clock + (uint32_t)config.baud_rate / 2) / (uint32_t)config.baud_rate;
    if (divisor < (oversampling == UART_OVERSAMPLING_8 ? 8u : 16u)
        || divisor > (oversampling == UART_OVERSAMPLING_8 ? 32767u : 65535u)) return UART_UNSUPPORTED;
    bool rx = config.direction != EMELA_UART_DIRECTION_TRANSMIT;
    bool tx = config.direction != EMELA_UART_DIRECTION_RECEIVE;
    int32_t pins[2];
    uint16_t count = 0;
    if (tx) pins[count++] = EMELA_UART2_TX_PIN;
    if (rx) pins[count++] = EMELA_UART2_RX_PIN;
    status = enter(id, false);
    if (status) return status;
    int32_t claim = stm32f4_pins_claim(STM32F4_OWNER_USART2, pins, count);
    if (claim) return leave(claim == 5 ? UART_RESOURCE_CONFLICT : UART_INVALID_ARGUMENT);

    uint32_t previous = stm32f4_interrupt_lock();
    NVIC_DisableIRQ(USART2_IRQn);
    __HAL_RCC_USART2_CLK_ENABLE();
    for (uint16_t index = 0; index < count; ++index) {
        uint32_t port = (uint32_t)pins[index] / 16;
        enable_gpio_clock(port);
        GPIO_InitTypeDef init = {.Pin = UINT32_C(1) << ((uint32_t)pins[index] % 16),
            .Mode = GPIO_MODE_AF_PP, .Pull = GPIO_NOPULL, .Speed = GPIO_SPEED_FREQ_HIGH,
            .Alternate = EMELA_UART2_AF};
        HAL_GPIO_Init(ports[port], &init);
    }
    uart.Instance = USART2;
    uart.Init = (UART_InitTypeDef){.BaudRate = (uint32_t)config.baud_rate,
        .WordLength = hardware_bits == 9 ? UART_WORDLENGTH_9B : UART_WORDLENGTH_8B,
        .StopBits = config.stop_bits == EMELA_UART_STOP_BITS_ONE ? UART_STOPBITS_1 : UART_STOPBITS_2,
        .Parity = parity,
        .Mode = (rx ? UART_MODE_RX : 0) | (tx ? UART_MODE_TX : 0),
        .HwFlowCtl = UART_HWCONTROL_NONE, .OverSampling = oversampling};
    HAL_StatusTypeDef result = HAL_UART_Init(&uart);
    if (result != HAL_OK) {
        stm32f4_interrupt_unlock(previous);
        return leave(UART_OTHER);
    }
    /* A new configuration starts with no subscriptions or unread old data. */
    __HAL_UART_DISABLE_IT(&uart, UART_IT_RXNE);
    __HAL_UART_DISABLE_IT(&uart, UART_IT_PE);
    __HAL_UART_DISABLE_IT(&uart, UART_IT_ERR);
    __HAL_UART_DISABLE_IT(&uart, UART_IT_TC);
    __HAL_UART_CLEAR_OREFLAG(&uart);
    NVIC_ClearPendingIRQ(USART2_IRQn);
    configured = true;
    can_receive = rx;
    can_transmit = tx;
    data_mask = (uint16_t)((UINT32_C(1) << bits) - 1);
    read_index = write_index = received_count = 0;
    receive_errors = 0;
    irq_enabled = notify_transmit = transmit_complete = false;
    receive_mode = EMELA_UART_RECEIVE_MODE_POLLING;
    for (uint16_t old = 0; old < claimed_count; ++old) {
        bool retained = false;
        for (uint16_t index = 0; index < count; ++index) retained |= claimed_pins[old] == pins[index];
        if (!retained) {
            HAL_GPIO_DeInit(ports[(uint32_t)claimed_pins[old] / 16], UINT32_C(1) << ((uint32_t)claimed_pins[old] % 16));
            stm32f4_pins_release(STM32F4_OWNER_USART2, &claimed_pins[old], 1);
        }
    }
    for (uint16_t index = 0; index < count; ++index) claimed_pins[index] = pins[index];
    claimed_count = count;
    stm32f4_interrupt_unlock(previous);
    return leave(UART_OK);
}

int32_t emela_uart_transmit(EmelaUartId id, const uint16_t *data, uint16_t length, int64_t timeout_ms) {
    if (!valid_timeout(timeout_ms) || (length && !data)) return UART_INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_transmit) return leave(UART_INVALID_STATE);
    for (uint16_t index = 0; index < length; ++index)
        if (data[index] > data_mask) return leave(UART_INVALID_ARGUMENT);
    if (!length) return leave(UART_OK);
    uint32_t start = HAL_GetTick();
    uint32_t timeout = (uint32_t)timeout_ms;
    uint32_t previous = stm32f4_interrupt_lock();
    transmit_complete = false;
    __HAL_UART_DISABLE_IT(&uart, UART_IT_TC);
    __HAL_UART_CLEAR_FLAG(&uart, UART_FLAG_TC);
    stm32f4_interrupt_unlock(previous);
    for (uint16_t index = 0; index < length; ++index) {
        while (!__HAL_UART_GET_FLAG(&uart, UART_FLAG_TXE)) {
            if (expired(start, timeout)) return leave(UART_TIMEOUT);
        }
        if (expired(start, timeout)) return leave(UART_TIMEOUT);
        uart.Instance->DR = data[index];
    }
    bool use_irq = irq_enabled && notify_transmit;
    if (use_irq) __HAL_UART_ENABLE_IT(&uart, UART_IT_TC);
    while (use_irq ? !transmit_complete : !__HAL_UART_GET_FLAG(&uart, UART_FLAG_TC)) {
        if (expired(start, timeout)) {
            __HAL_UART_DISABLE_IT(&uart, UART_IT_TC);
            return leave(UART_TIMEOUT);
        }
    }
    return leave(UART_OK);
}

/* Called with interrupts masked. Return one word, no word, or a typed error. */
static int32_t take_received(uint16_t *word, bool *available) {
    *available = false;
    if (receive_errors) {
        int32_t status = error_status(receive_errors);
        receive_errors = 0;
        return status;
    }
    if (received_count) {
        *word = received[read_index];
        read_index = (uint16_t)((read_index + 1) % RX_CAPACITY);
        received_count--;
        *available = true;
        return UART_OK;
    }
    if (receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) return UART_OK;
    uint32_t flags = uart.Instance->SR;
    if (flags & (RX_ERRORS | USART_SR_RXNE)) {
        uint16_t value = (uint16_t)uart.Instance->DR;
        if (flags & RX_ERRORS) return error_status(flags & RX_ERRORS);
        *word = value & data_mask;
        *available = true;
    }
    return UART_OK;
}

static int32_t receive_words(EmelaUartId id, int32_t size, int64_t timeout_ms,
                            uint16_t *out, uint16_t capacity, uint16_t *length, bool some) {
    if (length) *length = 0;
    if (size <= 0 || !out || !length || !valid_timeout(timeout_ms)) return UART_INVALID_ARGUMENT;
    if ((uint32_t)size > capacity) return UART_CAPACITY_EXCEEDED;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive || (receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE && !irq_enabled))
        return leave(UART_INVALID_STATE);
    uint32_t start = HAL_GetTick();
    uint16_t count = 0;
    while (count < (uint32_t)size) {
        if (expired(start, (uint32_t)timeout_ms)) return leave(UART_TIMEOUT);
        bool available;
        uint32_t previous = stm32f4_interrupt_lock();
        status = take_received(&out[count], &available);
        stm32f4_interrupt_unlock(previous);
        if (status) return leave(status);
        if (available) count++;
        else if (some && count) break;
        else if (expired(start, (uint32_t)timeout_ms)) return leave(UART_TIMEOUT);
    }
    *length = count;
    return leave(UART_OK);
}

int32_t emela_uart_receive(EmelaUartId id, int32_t size, int64_t timeout_ms,
                          uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout_ms, out, capacity, length, false);
}

int32_t emela_uart_receive_some(EmelaUartId id, int32_t size, int64_t timeout_ms,
                               uint16_t *out, uint16_t capacity, uint16_t *length) {
    return receive_words(id, size, timeout_ms, out, capacity, length, true);
}

int32_t emela_uart_discard_input(EmelaUartId id, int32_t max_size, int32_t *out) {
    if (max_size <= 0 || !out) return UART_INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(UART_INVALID_STATE);
    int32_t count = 0;
    uint32_t previous = stm32f4_interrupt_lock();
    receive_errors = 0;
    while (received_count && count < max_size) {
        read_index = (uint16_t)((read_index + 1) % RX_CAPACITY);
        received_count--;
        count++;
    }
    /* The STM32 data register has one pending word; never spin waiting for more. */
    uint32_t flags = uart.Instance->SR;
    if (count < max_size && (flags & (RX_ERRORS | USART_SR_RXNE))) {
        (void)uart.Instance->DR;
        if (flags & USART_SR_RXNE) count++;
    }
    stm32f4_interrupt_unlock(previous);
    *out = count;
    return leave(UART_OK);
}

int32_t emela_uart_set_receive_mode(EmelaUartId id, EmelaUartReceiveMode mode) {
    if (mode < EMELA_UART_RECEIVE_MODE_POLLING || mode > EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE)
        return UART_INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (!can_receive) return leave(UART_INVALID_STATE);
    uint32_t previous = stm32f4_interrupt_lock();
    receive_mode = mode;
    update_rx_interrupt();
    stm32f4_interrupt_unlock(previous);
    return leave(UART_OK);
}

int32_t emela_uart_enable_interrupt(EmelaUartId id, EmelaInterruptPriority priority, int32_t notify) {
    if (priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH
        || (notify != 0 && notify != 1)) return UART_INVALID_ARGUMENT;
    int32_t status = enter(id, true);
    if (status) return status;
    if (notify && !can_transmit) return leave(UART_INVALID_STATE);
    uint32_t previous = stm32f4_interrupt_lock();
    irq_enabled = true;
    notify_transmit = notify != 0;
    NVIC_SetPriority(USART2_IRQn, 12 - (uint32_t)priority * 4);
    NVIC_ClearPendingIRQ(USART2_IRQn);
    update_rx_interrupt();
    NVIC_EnableIRQ(USART2_IRQn);
    stm32f4_interrupt_unlock(previous);
    return leave(UART_OK);
}

int32_t emela_uart_disable_interrupt(EmelaUartId id) {
    int32_t status = enter(id, true);
    if (status) return status;
    uint32_t previous = stm32f4_interrupt_lock();
    irq_enabled = notify_transmit = false;
    update_rx_interrupt();
    __HAL_UART_DISABLE_IT(&uart, UART_IT_TC);
    NVIC_DisableIRQ(USART2_IRQn);
    NVIC_ClearPendingIRQ(USART2_IRQn);
    stm32f4_interrupt_unlock(previous);
    return leave(UART_OK);
}

void USART2_IRQHandler(void) {
    uint32_t flags = uart.Instance->SR;
    if (irq_enabled && receive_mode == EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE
        && (flags & (RX_ERRORS | USART_SR_RXNE))) {
        /* SR then DR clears PE/FE/NE/ORE; erroneous words are not returned as data. */
        uint16_t word = (uint16_t)uart.Instance->DR;
        if (flags & RX_ERRORS) receive_errors |= flags & RX_ERRORS;
        else if (flags & USART_SR_RXNE) {
            if (received_count == RX_CAPACITY) receive_errors |= USART_SR_ORE;
            else {
                received[write_index] = word & data_mask;
                write_index = (uint16_t)((write_index + 1) % RX_CAPACITY);
                received_count++;
            }
        }
    }
    if ((flags & USART_SR_TC) && __HAL_UART_GET_IT_SOURCE(&uart, UART_IT_TC)) {
        __HAL_UART_DISABLE_IT(&uart, UART_IT_TC);
        __HAL_UART_CLEAR_FLAG(&uart, UART_FLAG_TC);
        transmit_complete = true;
    }
    __DSB();
}
