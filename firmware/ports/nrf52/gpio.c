/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"
#include "gpio_support.h"
#include "emela_gpio.h"

enum { GPIO_OK, GPIO_INVALID_ARGUMENT, GPIO_NOT_CONFIGURED, GPIO_UNSUPPORTED,
       GPIO_BUSY, GPIO_RESOURCE_CONFLICT, GPIO_INVALID_STATE };
enum { CHANNEL_COUNT = 8 };
static const uint32_t analog_capable[EMELA_GPIO_PORT_COUNT] = EMELA_ANALOG_PIN_MASKS;
static volatile uint32_t configured[EMELA_GPIO_PORT_COUNT];
static volatile uint32_t outputs[EMELA_GPIO_PORT_COUNT];
static volatile uint32_t analog[EMELA_GPIO_PORT_COUNT];
static volatile uint8_t active_channels;
static volatile int32_t channel_pins[CHANNEL_COUNT];
static volatile EmelaInterruptPriority channel_priorities[CHANNEL_COUNT];

/* Called under the platform event lock, also used by the generated dispatcher. */
static int channel_for(int32_t pin) {
    for (unsigned i = 0; i < CHANNEL_COUNT; ++i)
        if ((active_channels & (1U << i)) && channel_pins[i] == pin) return (int)i;
    return -1;
}

static bool foreign_pin(int32_t pin, uint16_t owner) {
    uint16_t current = nrf52_pin_owner(pin);
    return current != 0 && current != owner;
}

static int32_t configure_owned(uint16_t owner, int32_t pin, EmelaGpioConfig config) {
    if (!nrf52_pin_valid(pin)) return GPIO_INVALID_ARGUMENT;
    if (nrf52_exception_number()) return GPIO_INVALID_STATE;
    EmelaGpioPull pull;
    EmelaGpioPinState initial = EMELA_GPIO_PIN_STATE_LOW;
    bool output = false, disconnected = false, open_drain = false;
    switch (config.tag) {
    case EMELA_GPIO_CONFIG_INPUT: pull = config.payload.input.pull; break;
    case EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL:
        output = true;
        pull = config.payload.output_push_pull.pull;
        initial = config.payload.output_push_pull.initial;
        break;
    case EMELA_GPIO_CONFIG_OUTPUT_OPEN_DRAIN:
        output = open_drain = true;
        pull = config.payload.output_open_drain.pull;
        initial = config.payload.output_open_drain.initial;
        break;
    case EMELA_GPIO_CONFIG_ANALOG:
        disconnected = true;
        pull = config.payload.analog.pull;
        break;
    default: return GPIO_INVALID_ARGUMENT;
    }
    if (pull < EMELA_GPIO_PULL_NO_PULL || pull > EMELA_GPIO_PULL_DOWN
        || initial < EMELA_GPIO_PIN_STATE_LOW || initial > EMELA_GPIO_PIN_STATE_HIGH)
        return GPIO_INVALID_ARGUMENT;
    uint32_t port = (uint32_t)pin / 32, index = (uint32_t)pin % 32;
    uint32_t bit = UINT32_C(1) << index;
    if (disconnected && (pull != EMELA_GPIO_PULL_NO_PULL || !(analog_capable[port] & bit)))
        return GPIO_UNSUPPORTED;
    uint32_t previous = nrf52_interrupt_lock();
    if (channel_for(pin) >= 0) { nrf52_interrupt_unlock(previous); return GPIO_BUSY; }
    int32_t status = nrf52_pins_claim(owner, &pin, 1);
    if (status) { nrf52_interrupt_unlock(previous); return status; }
    NRF_GPIO_Type *gpio = nrf52_gpio_port(port);
    gpio->DIRCLR = bit;
    if (initial == EMELA_GPIO_PIN_STATE_HIGH) gpio->OUTSET = bit;
    else gpio->OUTCLR = bit;
    uint32_t pull_bits = pull == EMELA_GPIO_PULL_UP ? GPIO_PIN_CNF_PULL_Pullup
        : pull == EMELA_GPIO_PULL_DOWN ? GPIO_PIN_CNF_PULL_Pulldown : GPIO_PIN_CNF_PULL_Disabled;
    gpio->PIN_CNF[index] = ((output ? GPIO_PIN_CNF_DIR_Output : GPIO_PIN_CNF_DIR_Input) << GPIO_PIN_CNF_DIR_Pos)
        | ((disconnected ? GPIO_PIN_CNF_INPUT_Disconnect : GPIO_PIN_CNF_INPUT_Connect) << GPIO_PIN_CNF_INPUT_Pos)
        | (pull_bits << GPIO_PIN_CNF_PULL_Pos)
        | ((open_drain ? GPIO_PIN_CNF_DRIVE_S0D1 : GPIO_PIN_CNF_DRIVE_S0S1) << GPIO_PIN_CNF_DRIVE_Pos);
    configured[port] |= bit;
    outputs[port] = output ? outputs[port] | bit : outputs[port] & ~bit;
    analog[port] = disconnected ? analog[port] | bit : analog[port] & ~bit;
    nrf52_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_configure(EmelaGpioPin pin, EmelaGpioConfig config) {
    return configure_owned(NRF52_OWNER_GPIO, pin.id, config);
}

int32_t nrf52_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial) {
    EmelaGpioConfig config = {.tag = EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL,
        .payload.output_push_pull = {.pull = EMELA_GPIO_PULL_NO_PULL, .initial = initial}};
    return configure_owned(owner, pin, config);
}

void nrf52_gpio_release_owned(uint16_t owner, int32_t pin) {
    uint32_t previous = nrf52_interrupt_lock();
    if (nrf52_pin_valid(pin) && nrf52_pin_owner(pin) == owner && channel_for(pin) < 0) {
        uint32_t port = (uint32_t)pin / 32, index = (uint32_t)pin % 32;
        uint32_t bit = UINT32_C(1) << index;
        nrf52_gpio_port(port)->PIN_CNF[index] = GPIO_PIN_CNF_INPUT_Disconnect << GPIO_PIN_CNF_INPUT_Pos;
        configured[port] &= ~bit;
        outputs[port] &= ~bit;
        analog[port] &= ~bit;
        nrf52_pins_release(owner, &pin, 1);
    }
    nrf52_interrupt_unlock(previous);
}

int32_t emela_gpio_read(EmelaGpioPin pin, EmelaGpioPinState *out) {
    if (!nrf52_pin_valid(pin.id) || !out) return GPIO_INVALID_ARGUMENT;
    uint32_t previous = nrf52_interrupt_lock();
    uint32_t port = (uint32_t)pin.id / 32, bit = UINT32_C(1) << ((uint32_t)pin.id % 32);
    int32_t status = foreign_pin(pin.id, NRF52_OWNER_GPIO) ? GPIO_RESOURCE_CONFLICT
        : !(configured[port] & bit) ? GPIO_NOT_CONFIGURED
        : (analog[port] & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    if (!status) *out = (nrf52_gpio_port(port)->IN & bit) ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
    nrf52_interrupt_unlock(previous);
    return status;
}

static int32_t write_owned(uint16_t owner, int32_t pin, int32_t level, bool toggle) {
    if (!nrf52_pin_valid(pin) || level < EMELA_GPIO_PIN_STATE_LOW || level > EMELA_GPIO_PIN_STATE_HIGH)
        return GPIO_INVALID_ARGUMENT;
    uint32_t previous = nrf52_interrupt_lock();
    uint32_t port = (uint32_t)pin / 32, bit = UINT32_C(1) << ((uint32_t)pin % 32);
    int32_t status = foreign_pin(pin, owner) ? GPIO_RESOURCE_CONFLICT
        : !(configured[port] & bit) ? GPIO_NOT_CONFIGURED
        : !(outputs[port] & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    if (!status) {
        NRF_GPIO_Type *gpio = nrf52_gpio_port(port);
        bool high = toggle ? !(gpio->OUT & bit) : level == EMELA_GPIO_PIN_STATE_HIGH;
        if (high) gpio->OUTSET = bit;
        else gpio->OUTCLR = bit;
    }
    nrf52_interrupt_unlock(previous);
    return status;
}

int32_t emela_gpio_write(EmelaGpioPin pin, EmelaGpioPinState level) {
    return write_owned(NRF52_OWNER_GPIO, pin.id, level, false);
}
int32_t nrf52_gpio_write_owned(uint16_t owner, int32_t pin, int32_t level) {
    return write_owned(owner, pin, level, false);
}
int32_t emela_gpio_toggle(EmelaGpioPin pin) {
    return write_owned(NRF52_OWNER_GPIO, pin.id, EMELA_GPIO_PIN_STATE_LOW, true);
}

int32_t emela_gpio_enable_interrupt(EmelaGpioPin pin, EmelaGpioEdge edge, EmelaInterruptPriority priority) {
    if (!nrf52_pin_valid(pin.id) || edge < EMELA_GPIO_EDGE_RISING || edge > EMELA_GPIO_EDGE_BOTH
        || priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH)
        return GPIO_INVALID_ARGUMENT;
    if (nrf52_exception_number()) return GPIO_INVALID_STATE;
    uint32_t previous = nrf52_interrupt_lock();
    uint32_t port = (uint32_t)pin.id / 32, index = (uint32_t)pin.id % 32, bit = UINT32_C(1) << index;
    int32_t status = foreign_pin(pin.id, NRF52_OWNER_GPIO) ? GPIO_RESOURCE_CONFLICT
        : !(configured[port] & bit) ? GPIO_NOT_CONFIGURED
        : ((outputs[port] | analog[port]) & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    int selected = channel_for(pin.id);
    for (int i = 0; !status && i < CHANNEL_COUNT; ++i) {
        if ((active_channels & (1U << i)) && i != selected && channel_priorities[i] != priority)
            status = GPIO_RESOURCE_CONFLICT;
    }
    if (selected < 0)
        for (int i = 0; i < CHANNEL_COUNT; ++i)
            if (!(active_channels & (1U << i))) { selected = i; break; }
    if (selected < 0) status = GPIO_RESOURCE_CONFLICT;
    if (status) { nrf52_interrupt_unlock(previous); return status; }
    uint32_t channel_bit = UINT32_C(1) << selected;
    NRF_GPIOTE->INTENCLR = channel_bit;
    NRF_GPIOTE->CONFIG[selected] = 0;
    NRF_GPIOTE->EVENTS_IN[selected] = 0;
    uint32_t polarity = edge == EMELA_GPIO_EDGE_RISING ? GPIOTE_CONFIG_POLARITY_LoToHi
        : edge == EMELA_GPIO_EDGE_FALLING ? GPIOTE_CONFIG_POLARITY_HiToLo : GPIOTE_CONFIG_POLARITY_Toggle;
    NRF_GPIOTE->CONFIG[selected] = (GPIOTE_CONFIG_MODE_Event << GPIOTE_CONFIG_MODE_Pos)
        | (index << GPIOTE_CONFIG_PSEL_Pos) | (port << GPIOTE_CONFIG_PORT_Pos)
        | (polarity << GPIOTE_CONFIG_POLARITY_Pos);
    if (!active_channels) NVIC_ClearPendingIRQ(GPIOTE_IRQn);
    channel_pins[selected] = pin.id;
    channel_priorities[selected] = priority;
    active_channels |= channel_bit;
    NVIC_SetPriority(GPIOTE_IRQn, 6U - 2U * (uint32_t)priority);
    NRF_GPIOTE->INTENSET = channel_bit;
    NVIC_EnableIRQ(GPIOTE_IRQn);
    __DSB();
    __ISB();
    nrf52_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_disable_interrupt(EmelaGpioPin pin) {
    if (!nrf52_pin_valid(pin.id)) return GPIO_INVALID_ARGUMENT;
    if (nrf52_exception_number()) return GPIO_INVALID_STATE;
    uint32_t previous = nrf52_interrupt_lock();
    if (foreign_pin(pin.id, NRF52_OWNER_GPIO)) { nrf52_interrupt_unlock(previous); return GPIO_RESOURCE_CONFLICT; }
    int channel = channel_for(pin.id);
    if (channel >= 0) {
        NRF_GPIOTE->INTENCLR = UINT32_C(1) << channel;
        NRF_GPIOTE->CONFIG[channel] = 0;
        NRF_GPIOTE->EVENTS_IN[channel] = 0;
        active_channels &= ~(1U << channel);
        if (!active_channels) {
            NVIC_DisableIRQ(GPIOTE_IRQn);
            NVIC_ClearPendingIRQ(GPIOTE_IRQn);
        }
        __DSB();
        __ISB();
    }
    nrf52_interrupt_unlock(previous);
    return GPIO_OK;
}

void GPIOTE_IRQHandler(void) {
    for (unsigned i = 0; i < CHANNEL_COUNT; ++i) {
        if (!(active_channels & (1U << i)) || !NRF_GPIOTE->EVENTS_IN[i]) continue;
        /* Acknowledge before entering generated/user code. */
        NRF_GPIOTE->EVENTS_IN[i] = 0;
        (void)NRF_GPIOTE->EVENTS_IN[i];
        uint32_t pin = (uint32_t)channel_pins[i];
        EmelaGpioPinState level = (nrf52_gpio_port(pin / 32)->IN & (UINT32_C(1) << (pin % 32)))
            ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
        emela_gpio_dispatch_interrupt((EmelaGpioPin){.id = (int32_t)pin}, level);
    }
    __DSB();
}
