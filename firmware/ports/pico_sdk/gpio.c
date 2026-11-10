/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"
#include "gpio_support.h"
#include "emela_gpio.h"
#include "hardware/gpio.h"
#include "hardware/irq.h"

enum { GPIO_OK, GPIO_INVALID_ARGUMENT, GPIO_NOT_CONFIGURED, GPIO_UNSUPPORTED,
       GPIO_BUSY, GPIO_RESOURCE_CONFLICT, GPIO_INVALID_STATE };
static volatile uint32_t configured;
static volatile uint32_t outputs;
static volatile uint32_t analog;
static volatile uint32_t open_drains;
/* Open drain HIGH releases the line; keep the requested level separate from IN. */
static volatile uint32_t output_levels;
static volatile uint32_t interrupt_pins;
static EmelaInterruptPriority interrupt_priority;

static bool foreign_pin(int32_t pin, uint16_t owner) {
    uint16_t current = emelino_pico_pin_owner(pin);
    return current != 0 && current != owner;
}

static int32_t configure_owned(uint16_t owner, int32_t pin, EmelaGpioConfig config) {
    if (!emelino_pico_pin_valid(pin)) return GPIO_INVALID_ARGUMENT;
    if (get_core_num() != 0 || emelino_pico_exception_number()) return GPIO_INVALID_STATE;
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
    uint32_t bit = UINT32_C(1) << pin;
    if (disconnected && (pull != EMELA_GPIO_PULL_NO_PULL || !(EMELA_ANALOG_PIN_MASK & bit)))
        return GPIO_UNSUPPORTED;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (interrupt_pins & bit) { emelino_pico_interrupt_unlock(previous); return GPIO_BUSY; }
    int32_t status = emelino_pico_pins_claim(owner, &pin, 1);
    if (status) { emelino_pico_interrupt_unlock(previous); return status; }
    gpio_init((uint)pin);
    gpio_set_pulls((uint)pin, pull == EMELA_GPIO_PULL_UP, pull == EMELA_GPIO_PULL_DOWN);
    if (disconnected) {
        gpio_set_function((uint)pin, GPIO_FUNC_NULL);
        gpio_set_input_enabled((uint)pin, false);
    } else {
        /* Load the latch before enabling the driver. Open drain never drives HIGH. */
        gpio_put((uint)pin, !open_drain && initial == EMELA_GPIO_PIN_STATE_HIGH);
        gpio_set_dir((uint)pin, output && (!open_drain || initial == EMELA_GPIO_PIN_STATE_LOW));
    }
    configured |= bit;
    outputs = output ? outputs | bit : outputs & ~bit;
    analog = disconnected ? analog | bit : analog & ~bit;
    open_drains = open_drain ? open_drains | bit : open_drains & ~bit;
    output_levels = initial == EMELA_GPIO_PIN_STATE_HIGH ? output_levels | bit : output_levels & ~bit;
    emelino_pico_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_configure(EmelaGpioPin pin, EmelaGpioConfig config) {
    return configure_owned(EMELINO_PICO_OWNER_GPIO, pin.id, config);
}

int32_t emelino_pico_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial) {
    EmelaGpioConfig config = {.tag = EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL,
        .payload.output_push_pull = {.pull = EMELA_GPIO_PULL_NO_PULL, .initial = initial}};
    return configure_owned(owner, pin, config);
}

void emelino_pico_gpio_release_owned(uint16_t owner, int32_t pin) {
    if (get_core_num() != 0 || emelino_pico_exception_number()) return;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (emelino_pico_pin_valid(pin) && emelino_pico_pin_owner(pin) == owner) {
        uint32_t bit = UINT32_C(1) << pin;
        if (!(interrupt_pins & bit)) {
            gpio_set_dir((uint)pin, GPIO_IN);
            gpio_set_function((uint)pin, GPIO_FUNC_NULL);
            gpio_set_input_enabled((uint)pin, false);
            gpio_disable_pulls((uint)pin);
            configured &= ~bit;
            outputs &= ~bit;
            analog &= ~bit;
            open_drains &= ~bit;
            output_levels &= ~bit;
            emelino_pico_pins_release(owner, &pin, 1);
        }
    }
    emelino_pico_interrupt_unlock(previous);
}

int32_t emela_gpio_read(EmelaGpioPin pin, EmelaGpioPinState *out) {
    if (!emelino_pico_pin_valid(pin.id) || !out) return GPIO_INVALID_ARGUMENT;
    if (get_core_num() != 0) return GPIO_INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    uint32_t bit = UINT32_C(1) << pin.id;
    int32_t status = foreign_pin(pin.id, EMELINO_PICO_OWNER_GPIO) ? GPIO_RESOURCE_CONFLICT
        : !(configured & bit) ? GPIO_NOT_CONFIGURED
        : (analog & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    if (!status) *out = gpio_get((uint)pin.id) ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
    emelino_pico_interrupt_unlock(previous);
    return status;
}

static int32_t write_owned(uint16_t owner, int32_t pin, int32_t level, bool toggle) {
    if (!emelino_pico_pin_valid(pin) || level < EMELA_GPIO_PIN_STATE_LOW || level > EMELA_GPIO_PIN_STATE_HIGH)
        return GPIO_INVALID_ARGUMENT;
    if (get_core_num() != 0) return GPIO_INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    uint32_t bit = UINT32_C(1) << pin;
    int32_t status = foreign_pin(pin, owner) ? GPIO_RESOURCE_CONFLICT
        : !(configured & bit) ? GPIO_NOT_CONFIGURED
        : !(outputs & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    if (!status) {
        bool high = toggle ? !(output_levels & bit) : level == EMELA_GPIO_PIN_STATE_HIGH;
        output_levels = high ? output_levels | bit : output_levels & ~bit;
        if (open_drains & bit) gpio_set_dir((uint)pin, !high);
        else gpio_put((uint)pin, high);
    }
    emelino_pico_interrupt_unlock(previous);
    return status;
}

int32_t emela_gpio_write(EmelaGpioPin pin, EmelaGpioPinState level) {
    return write_owned(EMELINO_PICO_OWNER_GPIO, pin.id, level, false);
}
int32_t emelino_pico_gpio_write_owned(uint16_t owner, int32_t pin, int32_t level) {
    return write_owned(owner, pin, level, false);
}
int32_t emela_gpio_toggle(EmelaGpioPin pin) {
    return write_owned(EMELINO_PICO_OWNER_GPIO, pin.id, EMELA_GPIO_PIN_STATE_LOW, true);
}

static void gpio_interrupt(uint pin, uint32_t events) {
    /* The SDK acknowledges latched events before invoking this callback. */
    if (pin >= 32 || !(interrupt_pins & (UINT32_C(1) << pin))
        || !(events & (GPIO_IRQ_EDGE_RISE | GPIO_IRQ_EDGE_FALL))) return;
    EmelaGpioPinState level = gpio_get(pin) ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
    emela_gpio_dispatch_interrupt((EmelaGpioPin){.id = (int32_t)pin}, level);
    __dsb();
}

int32_t emela_gpio_enable_interrupt(EmelaGpioPin pin, EmelaGpioEdge edge, EmelaInterruptPriority priority) {
    if (!emelino_pico_pin_valid(pin.id) || edge < EMELA_GPIO_EDGE_RISING || edge > EMELA_GPIO_EDGE_BOTH
        || priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH)
        return GPIO_INVALID_ARGUMENT;
    if (get_core_num() != 0 || emelino_pico_exception_number()) return GPIO_INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    uint32_t bit = UINT32_C(1) << pin.id;
    int32_t status = foreign_pin(pin.id, EMELINO_PICO_OWNER_GPIO) ? GPIO_RESOURCE_CONFLICT
        : !(configured & bit) ? GPIO_NOT_CONFIGURED
        : ((outputs | analog) & bit) ? GPIO_INVALID_STATE
        : ((interrupt_pins & ~bit) && interrupt_priority != priority) ? GPIO_RESOURCE_CONFLICT : GPIO_OK;
    if (status) { emelino_pico_interrupt_unlock(previous); return status; }
    uint32_t events = edge == EMELA_GPIO_EDGE_RISING ? GPIO_IRQ_EDGE_RISE
        : edge == EMELA_GPIO_EDGE_FALLING ? GPIO_IRQ_EDGE_FALL : GPIO_IRQ_EDGE_RISE | GPIO_IRQ_EDGE_FALL;
    gpio_set_irq_enabled((uint)pin.id, GPIO_IRQ_EDGE_RISE | GPIO_IRQ_EDGE_FALL, false);
    gpio_set_irq_callback(gpio_interrupt);
    if (!interrupt_pins) irq_clear(IO_IRQ_BANK0);
    interrupt_priority = priority;
    interrupt_pins |= bit;
    /* Values map to distinct supported hardware priorities on M0+ and M33. */
    irq_set_priority(IO_IRQ_BANK0, (uint8_t)(0xc0U - 0x40U * (uint32_t)priority));
    gpio_set_irq_enabled((uint)pin.id, events, true);
    irq_set_enabled(IO_IRQ_BANK0, true);
    __dsb();
    __isb();
    emelino_pico_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_disable_interrupt(EmelaGpioPin pin) {
    if (!emelino_pico_pin_valid(pin.id)) return GPIO_INVALID_ARGUMENT;
    if (get_core_num() != 0 || emelino_pico_exception_number()) return GPIO_INVALID_STATE;
    uint32_t previous = emelino_pico_interrupt_lock();
    if (foreign_pin(pin.id, EMELINO_PICO_OWNER_GPIO)) {
        emelino_pico_interrupt_unlock(previous);
        return GPIO_RESOURCE_CONFLICT;
    }
    uint32_t bit = UINT32_C(1) << pin.id;
    if (interrupt_pins & bit) {
        gpio_set_irq_enabled((uint)pin.id, GPIO_IRQ_EDGE_RISE | GPIO_IRQ_EDGE_FALL, false);
        interrupt_pins &= ~bit;
        if (!interrupt_pins) {
            irq_set_enabled(IO_IRQ_BANK0, false);
            irq_clear(IO_IRQ_BANK0);
            gpio_set_irq_callback(NULL);
        }
        __dsb();
        __isb();
    }
    emelino_pico_interrupt_unlock(previous);
    return GPIO_OK;
}
