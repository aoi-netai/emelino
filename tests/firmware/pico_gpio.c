/* SPDX-License-Identifier: Apache-2.0 */
#include <assert.h>
#include <string.h>
#include "bsp.h"
#include "gpio_support.h"
#include "emela_gpio.h"
#include "hardware/gpio.h"
#include "hardware/irq.h"

uint mock_core, mock_exception;
uint32_t mock_irq_mask;
static bool output[32], latch[32], input[32], input_enabled[32], pull_up[32], pull_down[32];
static uint function[32];
static uint32_t enabled_events[32], pending_events[32];
static gpio_irq_callback_t callback;
static bool irq_enabled;
static uint8_t irq_priority;
static unsigned dispatch_count;
static EmelaGpioPin dispatched_pin;
static EmelaGpioPinState dispatched_level;

void gpio_init(uint pin) {
    output[pin] = latch[pin] = false;
    function[pin] = GPIO_FUNC_SIO;
    input_enabled[pin] = true;
}
void gpio_set_pulls(uint pin, bool up, bool down) { pull_up[pin] = up; pull_down[pin] = down; }
void gpio_disable_pulls(uint pin) { gpio_set_pulls(pin, false, false); }
void gpio_set_function(uint pin, uint value) { function[pin] = value; input_enabled[pin] = true; }
void gpio_set_input_enabled(uint pin, bool value) { input_enabled[pin] = value; }
void gpio_put(uint pin, bool value) { latch[pin] = value; }
void gpio_set_dir(uint pin, bool value) { output[pin] = value; }
bool gpio_get(uint pin) { return output[pin] ? latch[pin] : input[pin]; }
void gpio_set_irq_enabled(uint pin, uint32_t events, bool enabled) {
    assert(!enabled || callback);
    pending_events[pin] &= ~events; /* SDK clears stale events on either transition. */
    if (enabled) enabled_events[pin] |= events;
    else enabled_events[pin] &= ~events;
}
void gpio_set_irq_callback(gpio_irq_callback_t value) { callback = value; }
void irq_clear(uint irq) { assert(irq == IO_IRQ_BANK0); }
void irq_set_priority(uint irq, uint8_t priority) { assert(irq == IO_IRQ_BANK0); irq_priority = priority; }
void irq_set_enabled(uint irq, bool value) { assert(irq == IO_IRQ_BANK0); irq_enabled = value; }

void emela_gpio_dispatch_interrupt(EmelaGpioPin pin, EmelaGpioPinState level) {
    assert(!pending_events[pin.id]); /* SDK acknowledgement precedes generated/user code. */
    ++dispatch_count;
    dispatched_pin = pin;
    dispatched_level = level;
}

static EmelaGpioPin pin(int32_t id) { return (EmelaGpioPin){.id = id}; }
static EmelaGpioConfig config(int32_t tag, int32_t pull, int32_t initial) {
    EmelaGpioConfig result = {.tag = tag};
    switch (tag) {
    case EMELA_GPIO_CONFIG_INPUT: result.payload.input.pull = pull; break;
    case EMELA_GPIO_CONFIG_ANALOG: result.payload.analog.pull = pull; break;
    case EMELA_GPIO_CONFIG_OUTPUT_OPEN_DRAIN:
        result.payload.output_open_drain.pull = pull;
        result.payload.output_open_drain.initial = initial;
        break;
    default:
        result.payload.output_push_pull.pull = pull;
        result.payload.output_push_pull.initial = initial;
        break;
    }
    return result;
}

static void pins_case(void) {
    for (int p = -1; p <= 32; ++p)
        assert(emelino_pico_pin_valid(p) == ((p >= 0 && p <= 22) || (p >= 25 && p <= 28)));
    int32_t held[] = {0, 1}, overlap[] = {2, 1}, invalid[] = {2, 23};
    assert(emelino_pico_pins_claim(EMELINO_PICO_OWNER_UART0, held, 2) == 0);
    assert(emelino_pico_pins_claim(EMELINO_PICO_OWNER_GPIO, overlap, 2) == 5);
    assert(emelino_pico_pin_owner(2) == 0);
    assert(emelino_pico_pins_claim(EMELINO_PICO_OWNER_GPIO, invalid, 2) == 1);
    assert(emelino_pico_pin_owner(2) == 0);
    emelino_pico_pins_release(EMELINO_PICO_OWNER_GPIO, held, 2);
    assert(emelino_pico_pin_owner(0) == EMELINO_PICO_OWNER_UART0);
    assert(emelino_pico_pins_claim(EMELINO_PICO_OWNER_UART0, held, 2) == 0);
    emelino_pico_pins_release(EMELINO_PICO_OWNER_UART0, held, 2);
    assert(emelino_pico_pin_owner(0) == 0 && emelino_pico_pin_owner(1) == 0);
    assert(emelino_pico_pins_claim(0, held, 2) == 1);
    assert(emelino_pico_pins_claim(1, NULL, 1) == 1);
    mock_irq_mask = 1;
    assert(emelino_pico_pins_claim(1, held, 2) == 0);
    assert(mock_irq_mask == 1);
    emelino_pico_pins_release(1, held, 2);
    assert(mock_irq_mask == 1);
}

static void output_case(void) {
    EmelaGpioPinState level = -1;
    assert(emela_gpio_read(pin(25), &level) == 2 && level == -1);
    assert(emela_gpio_configure(pin(25), config(1, 0, 1)) == 0);
    assert(output[25] && latch[25]);
    assert(emela_gpio_toggle(pin(25)) == 0 && !latch[25]);
    assert(emela_gpio_write(pin(25), 1) == 0 && latch[25]);
    assert(emela_gpio_read(pin(25), &level) == 0 && level == 1);
    assert(emela_gpio_write(pin(25), 2) == 1 && latch[25]);
    assert(emela_gpio_configure(pin(25), config(2, 1, 1)) == 0);
    assert(!output[25] && !latch[25] && pull_up[25]);
    input[25] = false; /* Another device is holding the released line LOW. */
    assert(emela_gpio_read(pin(25), &level) == 0 && level == 0);
    assert(emela_gpio_toggle(pin(25)) == 0 && output[25] && !latch[25]);
    assert(emela_gpio_toggle(pin(25)) == 0 && !output[25] && !latch[25]);
    assert(emela_gpio_write(pin(25), 0) == 0 && output[25]);
    assert(emela_gpio_configure(pin(25), config(0, 2, 0)) == 0);
    assert(!output[25] && pull_down[25] && !pull_up[25]);
    assert(emela_gpio_write(pin(25), 1) == 6);
    assert(emelino_pico_gpio_output_owned(16, 17, 1) == 0);
    assert(emela_gpio_write(pin(17), 0) == 5);
    assert(emela_gpio_configure(pin(17), config(0, 0, 0)) == 5);
    assert(emelino_pico_gpio_write_owned(16, 17, 0) == 0 && !latch[17]);
    emelino_pico_gpio_release_owned(17, 17);
    assert(emelino_pico_pin_owner(17) == 16);
    emelino_pico_gpio_release_owned(16, 17);
    assert(emelino_pico_pin_owner(17) == 0 && !output[17] && !input_enabled[17]);
    assert(emela_gpio_read(pin(17), &level) == 2);
}

static void analog_case(void) {
    EmelaGpioPinState level;
    assert(emela_gpio_configure(pin(25), config(3, 0, 0)) == 3);
    assert(emela_gpio_configure(pin(26), config(3, 1, 0)) == 3);
    assert(emelino_pico_pin_owner(25) == 0 && emelino_pico_pin_owner(26) == 0);
    assert(emela_gpio_configure(pin(26), config(3, 0, 0)) == 0);
    assert(!output[26] && !input_enabled[26] && function[26] == GPIO_FUNC_NULL);
    assert(emela_gpio_read(pin(26), &level) == 6);
    assert(emela_gpio_write(pin(26), 0) == 6);
    assert(emela_gpio_enable_interrupt(pin(26), 0, 1) == 6);
    assert(emela_gpio_configure(pin(26), config(0, 0, 0)) == 0 && input_enabled[26]);
    assert(emela_gpio_configure(pin(29), config(0, 0, 0)) == 1);
    assert(emela_gpio_configure(pin(26), config(0, 99, 0)) == 1);
}

static void interrupt_case(void) {
    for (int p = 2; p <= 12; ++p) {
        assert(emela_gpio_configure(pin(p), config(0, 0, 0)) == 0);
        assert(emela_gpio_enable_interrupt(pin(p), 2, 1) == 0);
    }
    assert(irq_enabled && irq_priority == 0x80);
    assert(emela_gpio_configure(pin(2), config(1, 0, 1)) == 4);
    assert(emela_gpio_enable_interrupt(pin(2), 0, 2) == 5);
    assert(enabled_events[2] == (GPIO_IRQ_EDGE_RISE | GPIO_IRQ_EDGE_FALL));
    input[2] = true;
    pending_events[2] = GPIO_IRQ_EDGE_RISE;
    uint32_t events = pending_events[2] & enabled_events[2];
    pending_events[2] &= ~events;
    mock_exception = 29;
    callback(2, events);
    mock_exception = 0;
    assert(dispatch_count == 1 && dispatched_pin.id == 2 && dispatched_level == 1);
    for (int p = 3; p <= 12; ++p) assert(emela_gpio_disable_interrupt(pin(p)) == 0);
    assert(irq_enabled);
    assert(emela_gpio_enable_interrupt(pin(2), 1, 2) == 0);
    assert(enabled_events[2] == GPIO_IRQ_EDGE_FALL && irq_priority == 0x40);
    assert(emela_gpio_disable_interrupt(pin(2)) == 0 && !irq_enabled && !callback);
    assert(emela_gpio_disable_interrupt(pin(2)) == 0);
    pending_events[2] = GPIO_IRQ_EDGE_RISE;
    assert(emela_gpio_enable_interrupt(pin(2), 0, 0) == 0);
    assert(!pending_events[2] && irq_enabled && irq_priority == 0xc0);
    assert(emela_gpio_disable_interrupt(pin(2)) == 0);
    assert(emela_gpio_configure(pin(2), config(1, 0, 0)) == 0);
    assert(emela_gpio_enable_interrupt(pin(2), 0, 1) == 6);
}

static void context_case(void) {
    EmelaGpioPinState level;
    assert(emela_gpio_configure(pin(25), config(1, 0, 0)) == 0);
    mock_exception = 29;
    assert(emela_gpio_configure(pin(25), config(0, 0, 0)) == 6);
    assert(emela_gpio_enable_interrupt(pin(25), 0, 1) == 6);
    assert(emela_gpio_disable_interrupt(pin(25)) == 6);
    assert(emela_gpio_write(pin(25), 1) == 0);
    assert(emela_gpio_read(pin(25), &level) == 0 && level == 1);
    mock_irq_mask = 1;
    assert(emela_gpio_toggle(pin(25)) == 0 && mock_irq_mask == 1);
    mock_exception = 0;
    mock_core = 1;
    assert(emela_gpio_configure(pin(25), config(0, 0, 0)) == 6);
    assert(emela_gpio_read(pin(25), &level) == 6);
    assert(emela_gpio_write(pin(25), 1) == 6);
    assert(emela_gpio_toggle(pin(25)) == 6);
}

int main(int argc, char **argv) {
    assert(argc == 2);
    if (!strcmp(argv[1], "pins")) pins_case();
    else if (!strcmp(argv[1], "output")) output_case();
    else if (!strcmp(argv[1], "analog")) analog_case();
    else if (!strcmp(argv[1], "interrupt")) interrupt_case();
    else if (!strcmp(argv[1], "context")) context_case();
    else assert(false);
    return 0;
}
