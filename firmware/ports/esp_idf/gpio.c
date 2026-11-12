/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"
#include "gpio_support.h"
#include "emela_gpio.h"
#include "driver/gpio.h"
#include "esp_intr_alloc.h"

enum { OK, INVALID_ARGUMENT, NOT_CONFIGURED, UNSUPPORTED, BUSY, RESOURCE_CONFLICT, INVALID_STATE };
static uint32_t configured, outputs, analog, output_levels, interrupt_pins;
static bool occupied, service_installed;
static EmelaInterruptPriority interrupt_priority;

static int32_t enter(void) {
    if (!emelino_idf_task_context()) return INVALID_STATE;
    uint32_t previous = emelino_idf_lock();
    int32_t status = occupied ? BUSY : OK;
    if (!status) occupied = true;
    emelino_idf_unlock(previous);
    return status;
}
static int32_t leave(int32_t status) {
    uint32_t previous = emelino_idf_lock();
    occupied = false;
    emelino_idf_unlock(previous);
    return status;
}
static bool foreign_pin(int32_t pin, uint16_t owner) {
    uint16_t current = emelino_idf_pin_owner(pin);
    return current && current != owner;
}
static int32_t configure_owned(uint16_t owner, int32_t pin, EmelaGpioConfig config) {
    if (!emelino_idf_pin_valid(pin)) return INVALID_ARGUMENT;
    EmelaGpioPull pull;
    EmelaGpioPinState initial = EMELA_GPIO_PIN_STATE_LOW;
    gpio_mode_t mode;
    switch (config.tag) {
    case EMELA_GPIO_CONFIG_INPUT: pull = config.payload.input.pull; mode = GPIO_MODE_INPUT; break;
    case EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL:
        pull = config.payload.output_push_pull.pull; initial = config.payload.output_push_pull.initial;
        mode = GPIO_MODE_INPUT_OUTPUT; break;
    case EMELA_GPIO_CONFIG_OUTPUT_OPEN_DRAIN:
        pull = config.payload.output_open_drain.pull; initial = config.payload.output_open_drain.initial;
        mode = GPIO_MODE_INPUT_OUTPUT_OD; break;
    case EMELA_GPIO_CONFIG_ANALOG: pull = config.payload.analog.pull; mode = GPIO_MODE_DISABLE; break;
    default: return INVALID_ARGUMENT;
    }
    if (pull < EMELA_GPIO_PULL_NO_PULL || pull > EMELA_GPIO_PULL_DOWN
        || initial < EMELA_GPIO_PIN_STATE_LOW || initial > EMELA_GPIO_PIN_STATE_HIGH) return INVALID_ARGUMENT;
    uint32_t bit = UINT32_C(1) << pin;
    if (mode == GPIO_MODE_DISABLE && (pull != EMELA_GPIO_PULL_NO_PULL || !(EMELA_ANALOG_PIN_MASK & bit)))
        return UNSUPPORTED;
    int32_t status = enter();
    if (status) return status;
    if (interrupt_pins & bit) return leave(BUSY);
    bool was_owned = emelino_idf_pin_owner(pin) == owner;
    status = emelino_idf_pins_claim(owner, &pin, 1);
    if (status) return leave(status);
    gpio_config_t native = {.pin_bit_mask = UINT64_C(1) << pin, .mode = mode,
        .pull_up_en = pull == EMELA_GPIO_PULL_UP, .pull_down_en = pull == EMELA_GPIO_PULL_DOWN,
        .intr_type = GPIO_INTR_DISABLE};
    uint32_t previous = emelino_idf_lock();
    /* Set the output latch before enabling either push-pull or open-drain. */
    esp_err_t error = gpio_set_level(pin, initial == EMELA_GPIO_PIN_STATE_HIGH);
    if (!error) error = gpio_config(&native);
    if (!error) {
        configured |= bit;
        outputs = (mode & GPIO_MODE_DEF_OUTPUT) ? outputs | bit : outputs & ~bit;
        analog = mode == GPIO_MODE_DISABLE ? analog | bit : analog & ~bit;
        output_levels = initial == EMELA_GPIO_PIN_STATE_HIGH ? output_levels | bit : output_levels & ~bit;
    }
    emelino_idf_unlock(previous);
    if (error && !was_owned) emelino_idf_pins_release(owner, &pin, 1);
    return leave(error ? INVALID_STATE : OK);
}
int32_t emela_gpio_configure(EmelaGpioPin pin, EmelaGpioConfig config) {
    return configure_owned(EMELINO_IDF_OWNER_GPIO, pin.id, config);
}
int32_t emelino_idf_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial) {
    EmelaGpioConfig config = {.tag = EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL,
        .payload.output_push_pull = {.pull = EMELA_GPIO_PULL_NO_PULL, .initial = initial}};
    return configure_owned(owner, pin, config);
}
void emelino_idf_gpio_release_owned(uint16_t owner, int32_t pin) {
    if (!emelino_idf_task_context() || !emelino_idf_pin_valid(pin)) return;
    uint32_t previous = emelino_idf_lock();
    uint32_t bit = UINT32_C(1) << pin;
    if (emelino_idf_pin_owner(pin) == owner && !(interrupt_pins & bit)) {
        gpio_reset_pin(pin);
        configured &= ~bit; outputs &= ~bit; analog &= ~bit; output_levels &= ~bit;
        emelino_idf_pins_release(owner, &pin, 1);
    }
    emelino_idf_unlock(previous);
}
int32_t emela_gpio_read(EmelaGpioPin pin, EmelaGpioPinState *out) {
    if (!emelino_idf_pin_valid(pin.id) || !out) return INVALID_ARGUMENT;
    uint32_t previous = emelino_idf_lock();
    uint32_t bit = UINT32_C(1) << pin.id;
    int32_t status = foreign_pin(pin.id, EMELINO_IDF_OWNER_GPIO) ? RESOURCE_CONFLICT
        : !(configured & bit) ? NOT_CONFIGURED : (analog & bit) ? INVALID_STATE : OK;
    if (!status) *out = gpio_get_level(pin.id) ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
    emelino_idf_unlock(previous);
    return status;
}
static int32_t write_owned(uint16_t owner, int32_t pin, int32_t level, bool toggle) {
    if (!emelino_idf_pin_valid(pin) || level < EMELA_GPIO_PIN_STATE_LOW || level > EMELA_GPIO_PIN_STATE_HIGH)
        return INVALID_ARGUMENT;
    uint32_t previous = emelino_idf_lock();
    uint32_t bit = UINT32_C(1) << pin;
    int32_t status = foreign_pin(pin, owner) ? RESOURCE_CONFLICT
        : !(configured & bit) ? NOT_CONFIGURED : !(outputs & bit) ? INVALID_STATE : OK;
    if (!status) {
        bool high = toggle ? !(output_levels & bit) : level == EMELA_GPIO_PIN_STATE_HIGH;
        if (gpio_set_level(pin, high) != ESP_OK) status = INVALID_STATE;
        else output_levels = high ? output_levels | bit : output_levels & ~bit;
    }
    emelino_idf_unlock(previous);
    return status;
}
int32_t emela_gpio_write(EmelaGpioPin pin, EmelaGpioPinState state) {
    return write_owned(EMELINO_IDF_OWNER_GPIO, pin.id, state, false);
}
int32_t emela_gpio_toggle(EmelaGpioPin pin) {
    return write_owned(EMELINO_IDF_OWNER_GPIO, pin.id, EMELA_GPIO_PIN_STATE_LOW, true);
}
int32_t emelino_idf_gpio_write_owned(uint16_t owner, int32_t pin, int32_t state) {
    return write_owned(owner, pin, state, false);
}
static void gpio_interrupt(void *argument) {
    int32_t pin = (int32_t)(uintptr_t)argument;
    if (interrupt_pins & (UINT32_C(1) << pin))
        emela_gpio_dispatch_interrupt((EmelaGpioPin){.id = pin}, gpio_get_level(pin)
            ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW);
}
int32_t emela_gpio_enable_interrupt(EmelaGpioPin pin, EmelaGpioEdge edge, EmelaInterruptPriority priority) {
    if (!emelino_idf_pin_valid(pin.id) || edge < EMELA_GPIO_EDGE_RISING || edge > EMELA_GPIO_EDGE_BOTH
        || priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH) return INVALID_ARGUMENT;
    int32_t status = enter();
    if (status) return status;
    uint32_t bit = UINT32_C(1) << pin.id;
    if (foreign_pin(pin.id, EMELINO_IDF_OWNER_GPIO)) return leave(RESOURCE_CONFLICT);
    if (!(configured & bit)) return leave(NOT_CONFIGURED);
    if ((outputs | analog) & bit) return leave(INVALID_STATE);
    if (service_installed && interrupt_priority != priority) return leave(RESOURCE_CONFLICT);
    if (!service_installed) {
        int flags = priority == EMELA_INTERRUPT_PRIORITY_LOW ? ESP_INTR_FLAG_LEVEL1
            : priority == EMELA_INTERRUPT_PRIORITY_NORMAL ? ESP_INTR_FLAG_LEVEL2 : ESP_INTR_FLAG_LEVEL3;
        /* Do not take over a GPIO ISR service installed by another component. */
        if (gpio_install_isr_service(flags) != ESP_OK) return leave(RESOURCE_CONFLICT);
        service_installed = true;
        interrupt_priority = priority;
    }
    gpio_int_type_t native = edge == EMELA_GPIO_EDGE_RISING ? GPIO_INTR_POSEDGE
        : edge == EMELA_GPIO_EDGE_FALLING ? GPIO_INTR_NEGEDGE : GPIO_INTR_ANYEDGE;
    uint32_t previous = emelino_idf_lock();
    gpio_intr_disable(pin.id);
    if (gpio_set_intr_type(pin.id, native) != ESP_OK
        || gpio_isr_handler_add(pin.id, gpio_interrupt, (void *)(uintptr_t)pin.id) != ESP_OK) {
        emelino_idf_unlock(previous);
        if (!interrupt_pins) { gpio_uninstall_isr_service(); service_installed = false; }
        return leave(INVALID_STATE);
    }
    interrupt_pins |= bit;
    if (gpio_intr_enable(pin.id) != ESP_OK) {
        interrupt_pins &= ~bit;
        gpio_isr_handler_remove(pin.id);
        emelino_idf_unlock(previous);
        if (!interrupt_pins) { gpio_uninstall_isr_service(); service_installed = false; }
        return leave(INVALID_STATE);
    }
    emelino_idf_unlock(previous);
    return leave(OK);
}
int32_t emela_gpio_disable_interrupt(EmelaGpioPin pin) {
    if (!emelino_idf_pin_valid(pin.id)) return INVALID_ARGUMENT;
    int32_t status = enter();
    if (status) return status;
    if (foreign_pin(pin.id, EMELINO_IDF_OWNER_GPIO)) return leave(RESOURCE_CONFLICT);
    uint32_t bit = UINT32_C(1) << pin.id;
    if (interrupt_pins & bit) {
        gpio_intr_disable(pin.id);
        interrupt_pins &= ~bit;
        gpio_isr_handler_remove(pin.id);
        if (!interrupt_pins) { gpio_uninstall_isr_service(); service_installed = false; }
    }
    return leave(OK);
}
