/* SPDX-License-Identifier: Apache-2.0 */
#include <stdbool.h>
#include <stdint.h>

#include "board_config.h"
#include "emela_gpio.h"
#include "registers.h"
#include "bsp.h"
#include "gpio_support.h"

/* Public GPIO status: 0 succeeds; failures are GpioErrorKind + 1. */
enum {
    GPIO_OK = 0,
    GPIO_INVALID_ARGUMENT = 1,
    GPIO_NOT_CONFIGURED = 2,
    GPIO_UNSUPPORTED = 3,
    GPIO_BUSY = 4,
    GPIO_RESOURCE_CONFLICT = 5,
    GPIO_INVALID_STATE = 6,
};

/* Shared with ISR reads; configuration changes are serialized by PRIMASK. */
static volatile uint16_t configured_pins[EMELA_GPIO_PORT_COUNT];
static volatile uint16_t output_pins[EMELA_GPIO_PORT_COUNT];
static volatile uint16_t analog_pins[EMELA_GPIO_PORT_COUNT];
static volatile uint16_t interrupt_lines;
static volatile uint8_t interrupt_ports[16];
static volatile uint8_t interrupt_priorities[16];

static bool valid_pin(EmelaGpioPin pin) { return stm32f4_pin_valid(pin.id); }
static bool foreign_pin(EmelaGpioPin pin, uint16_t owner) {
    uint16_t current = stm32f4_pin_owner(pin.id);
    return current != 0 && current != owner;
}

static IRQn_Type irq_number(uint32_t line) {
    static const IRQn_Type individual[] = {EXTI0_IRQn, EXTI1_IRQn, EXTI2_IRQn, EXTI3_IRQn, EXTI4_IRQn};
    return line < 5 ? individual[line] : line < 10 ? EXTI9_5_IRQn : EXTI15_10_IRQn;
}


static uint32_t irq_lines(uint32_t line) {
    return line < 5 ? UINT32_C(1) << line : line < 10 ? UINT32_C(0x03e0) : UINT32_C(0xfc00);
}

static bool interrupt_owns(uint32_t port, uint32_t line) {
    return (interrupt_lines & (UINT32_C(1) << line)) && interrupt_ports[line] == port;
}

static int32_t configure_owned(uint16_t owner, EmelaGpioPin pin, EmelaGpioConfig config) {
    if (!valid_pin(pin)) return GPIO_INVALID_ARGUMENT;
    if (stm32f4_exception_number() != 0) return GPIO_INVALID_STATE;
    EmelaGpioPull pull;
    EmelaGpioPinState initial = EMELA_GPIO_PIN_STATE_LOW;
    uint32_t mode;
    switch (config.tag) {
    case EMELA_GPIO_CONFIG_INPUT:
        pull = config.payload.input.pull;
        mode = GPIO_MODE_INPUT;
        break;
    case EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL:
        pull = config.payload.output_push_pull.pull;
        initial = config.payload.output_push_pull.initial;
        mode = GPIO_MODE_OUTPUT_PP;
        break;
    case EMELA_GPIO_CONFIG_OUTPUT_OPEN_DRAIN:
        pull = config.payload.output_open_drain.pull;
        initial = config.payload.output_open_drain.initial;
        mode = GPIO_MODE_OUTPUT_OD;
        break;
    case EMELA_GPIO_CONFIG_ANALOG:
        pull = config.payload.analog.pull;
        mode = GPIO_MODE_ANALOG;
        break;
    default:
        return GPIO_INVALID_ARGUMENT;
    }
    if (pull < EMELA_GPIO_PULL_NO_PULL || pull > EMELA_GPIO_PULL_DOWN
        || initial < EMELA_GPIO_PIN_STATE_LOW || initial > EMELA_GPIO_PIN_STATE_HIGH)
        return GPIO_INVALID_ARGUMENT;
    /* F4 disables the digital pull resistors in analog mode. */
    if (mode == GPIO_MODE_ANALOG && pull != EMELA_GPIO_PULL_NO_PULL) return GPIO_UNSUPPORTED;

    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t line = (uint32_t)pin.id % 16;
    uint32_t bit = UINT32_C(1) << line;
    uint32_t shift = line * 2;
    uint32_t previous = stm32f4_interrupt_lock();
    if (interrupt_owns(port, line)) {
        stm32f4_interrupt_unlock(previous);
        return GPIO_BUSY;
    }
    int32_t claim = stm32f4_pins_claim(owner, &pin.id, 1);
    if (claim != GPIO_OK) { stm32f4_interrupt_unlock(previous); return claim; }
    stm32f4_enable_gpio_clock(port);
    GPIO_TypeDef *gpio = stm32f4_gpio_port(port);
    /* Disconnect a previous output while changing its type and initial level. */
    CLEAR_BIT(gpio->MODER, GPIO_MODER_MODER0 << shift);
    HAL_GPIO_WritePin(gpio, (uint16_t)bit, initial == EMELA_GPIO_PIN_STATE_HIGH ? GPIO_PIN_SET : GPIO_PIN_RESET);
    GPIO_InitTypeDef init = {.Pin = bit, .Mode = mode, .Pull = (uint32_t)pull, .Speed = GPIO_SPEED_FREQ_LOW};
    HAL_GPIO_Init(gpio, &init);
    configured_pins[port] |= bit;
    bool is_output = mode == GPIO_MODE_OUTPUT_PP || mode == GPIO_MODE_OUTPUT_OD;
    output_pins[port] = is_output ? output_pins[port] | bit : output_pins[port] & ~bit;
    analog_pins[port] = mode == GPIO_MODE_ANALOG ? analog_pins[port] | bit : analog_pins[port] & ~bit;
    stm32f4_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_configure(EmelaGpioPin pin, EmelaGpioConfig config) {
    return configure_owned(STM32F4_OWNER_GPIO, pin, config);
}

int32_t stm32f4_gpio_output_owned(uint16_t owner, int32_t pin, int32_t initial) {
    EmelaGpioConfig config = {.tag = EMELA_GPIO_CONFIG_OUTPUT_PUSH_PULL,
        .payload.output_push_pull = {.pull = EMELA_GPIO_PULL_NO_PULL, .initial = initial}};
    return configure_owned(owner, (EmelaGpioPin){pin}, config);
}

void stm32f4_gpio_release_owned(uint16_t owner, int32_t pin) {
    uint32_t previous = stm32f4_interrupt_lock();
    if (stm32f4_pin_valid(pin) && stm32f4_pin_owner(pin) == owner) {
        uint32_t port = (uint32_t)pin / 16, bit = 1U << ((uint32_t)pin % 16);
        if (!(interrupt_lines & bit) || interrupt_ports[(uint32_t)pin % 16] != port) {
            HAL_GPIO_DeInit(stm32f4_gpio_port(port), bit);
            configured_pins[port] &= ~bit;
            output_pins[port] &= ~bit;
            analog_pins[port] &= ~bit;
            stm32f4_pins_release(owner, &pin, 1);
        }
    }
    stm32f4_interrupt_unlock(previous);
}

int32_t emela_gpio_read(EmelaGpioPin pin, EmelaGpioPinState *out) {
    if (!valid_pin(pin) || out == 0) return GPIO_INVALID_ARGUMENT;
    if (foreign_pin(pin, STM32F4_OWNER_GPIO)) return GPIO_RESOURCE_CONFLICT;
    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t bit = UINT32_C(1) << ((uint32_t)pin.id % 16);
    uint32_t previous = stm32f4_interrupt_lock();
    int32_t status = !(configured_pins[port] & bit) ? GPIO_NOT_CONFIGURED
        : analog_pins[port] & bit ? GPIO_INVALID_STATE : GPIO_OK;
    if (status == GPIO_OK)
        *out = HAL_GPIO_ReadPin(stm32f4_gpio_port(port), (uint16_t)bit) == GPIO_PIN_SET
            ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
    stm32f4_interrupt_unlock(previous);
    return status;
}

static int32_t write_owned(uint16_t owner, EmelaGpioPin pin, EmelaGpioPinState state) {
    if (!valid_pin(pin) || state < EMELA_GPIO_PIN_STATE_LOW || state > EMELA_GPIO_PIN_STATE_HIGH)
        return GPIO_INVALID_ARGUMENT;
    if (foreign_pin(pin, owner)) return GPIO_RESOURCE_CONFLICT;
    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t bit = UINT32_C(1) << ((uint32_t)pin.id % 16);
    uint32_t previous = stm32f4_interrupt_lock();
    int32_t status = !(configured_pins[port] & bit) ? GPIO_NOT_CONFIGURED
        : !(output_pins[port] & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    if (status == GPIO_OK)
        HAL_GPIO_WritePin(stm32f4_gpio_port(port), (uint16_t)bit,
                         state == EMELA_GPIO_PIN_STATE_HIGH ? GPIO_PIN_SET : GPIO_PIN_RESET);
    stm32f4_interrupt_unlock(previous);
    return status;
}

int32_t emela_gpio_write(EmelaGpioPin pin, EmelaGpioPinState state) {
    return write_owned(STM32F4_OWNER_GPIO, pin, state);
}

int32_t stm32f4_gpio_write_owned(uint16_t owner, int32_t pin, int32_t state) {
    return write_owned(owner, (EmelaGpioPin){pin}, state);
}

int32_t emela_gpio_toggle(EmelaGpioPin pin) {
    if (!valid_pin(pin)) return GPIO_INVALID_ARGUMENT;
    if (foreign_pin(pin, STM32F4_OWNER_GPIO)) return GPIO_RESOURCE_CONFLICT;
    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t bit = UINT32_C(1) << ((uint32_t)pin.id % 16);
    uint32_t previous = stm32f4_interrupt_lock();
    int32_t status = !(configured_pins[port] & bit) ? GPIO_NOT_CONFIGURED
        : !(output_pins[port] & bit) ? GPIO_INVALID_STATE : GPIO_OK;
    /* The read and BSRR update are one critical section, including ISR callers. */
    if (status == GPIO_OK) HAL_GPIO_TogglePin(stm32f4_gpio_port(port), (uint16_t)bit);
    stm32f4_interrupt_unlock(previous);
    return status;
}

int32_t emela_gpio_enable_interrupt(EmelaGpioPin pin, EmelaGpioEdge edge, EmelaInterruptPriority priority) {
    if (!valid_pin(pin) || edge < EMELA_GPIO_EDGE_RISING || edge > EMELA_GPIO_EDGE_BOTH
        || priority < EMELA_INTERRUPT_PRIORITY_LOW || priority > EMELA_INTERRUPT_PRIORITY_HIGH)
        return GPIO_INVALID_ARGUMENT;
    if (stm32f4_exception_number() != 0) return GPIO_INVALID_STATE;
    if (foreign_pin(pin, STM32F4_OWNER_GPIO)) return GPIO_RESOURCE_CONFLICT;
    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t line = (uint32_t)pin.id % 16;
    uint32_t bit = UINT32_C(1) << line;
    uint32_t group = irq_lines(line);
    uint32_t previous = stm32f4_interrupt_lock();
    int32_t status = GPIO_OK;
    if (!(configured_pins[port] & bit)) status = GPIO_NOT_CONFIGURED;
    else if ((output_pins[port] | analog_pins[port]) & bit) status = GPIO_INVALID_STATE;
    else if ((interrupt_lines & bit) && interrupt_ports[line] != port) status = GPIO_RESOURCE_CONFLICT;
    else {
        for (uint32_t other = 0; other < 16; ++other) {
            if ((interrupt_lines & group & ~bit & (UINT32_C(1) << other))
                && interrupt_priorities[other] != priority) {
                status = GPIO_RESOURCE_CONFLICT;
                break;
            }
        }
    }
    if (status != GPIO_OK) {
        stm32f4_interrupt_unlock(previous);
        return status;
    }

    IRQn_Type irq = irq_number(line);
    GPIO_TypeDef *gpio = stm32f4_gpio_port(port);
    CLEAR_BIT(EXTI->IMR, bit);
    GPIO_InitTypeDef init = {
        .Pin = bit,
        .Mode = edge == EMELA_GPIO_EDGE_RISING ? GPIO_MODE_IT_RISING
            : edge == EMELA_GPIO_EDGE_FALLING ? GPIO_MODE_IT_FALLING : GPIO_MODE_IT_RISING_FALLING,
        .Pull = (gpio->PUPDR >> (line * 2)) & GPIO_PUPDR_PUPDR0,
    };
    HAL_GPIO_Init(gpio, &init);
    __HAL_GPIO_EXTI_CLEAR_IT(bit); /* Discard pending edges from the old subscription. */
    if (!(interrupt_lines & group)) NVIC_ClearPendingIRQ(irq);
    interrupt_ports[line] = (uint8_t)port;
    interrupt_priorities[line] = (uint8_t)priority;
    interrupt_lines |= bit;
    /* Four implemented priority bits; smaller hardware values preempt larger ones. */
    NVIC_SetPriority(irq, 12 - (uint32_t)priority * 4);
    NVIC_EnableIRQ(irq);
    __DSB();
    __ISB();
    stm32f4_interrupt_unlock(previous);
    return GPIO_OK;
}

int32_t emela_gpio_disable_interrupt(EmelaGpioPin pin) {
    if (!valid_pin(pin)) return GPIO_INVALID_ARGUMENT;
    if (stm32f4_exception_number() != 0) return GPIO_INVALID_STATE;
    if (foreign_pin(pin, STM32F4_OWNER_GPIO)) return GPIO_RESOURCE_CONFLICT;
    uint32_t port = (uint32_t)pin.id / 16;
    uint32_t line = (uint32_t)pin.id % 16;
    uint32_t bit = UINT32_C(1) << line;
    uint32_t previous = stm32f4_interrupt_lock();
    if (interrupt_owns(port, line)) {
        CLEAR_BIT(EXTI->IMR, bit);
        CLEAR_BIT(EXTI->RTSR, bit);
        CLEAR_BIT(EXTI->FTSR, bit);
        __HAL_GPIO_EXTI_CLEAR_IT(bit);
        interrupt_lines &= ~bit;
        if (!(interrupt_lines & irq_lines(line))) {
            IRQn_Type irq = irq_number(line);
            NVIC_DisableIRQ(irq);
            NVIC_ClearPendingIRQ(irq);
        }
        __DSB();
        __ISB();
    }
    stm32f4_interrupt_unlock(previous);
    return GPIO_OK;
}

/* HAL acknowledges the EXTI source before invoking this strong callback. */
void HAL_GPIO_EXTI_Callback(uint16_t pin_mask) {
    for (uint32_t line = 0; line < 16; ++line) {
        uint32_t bit = UINT32_C(1) << line;
        if (!(pin_mask & interrupt_lines & bit)) continue;
        uint32_t port = interrupt_ports[line];
        EmelaGpioPinState level = HAL_GPIO_ReadPin(stm32f4_gpio_port(port), (uint16_t)bit) == GPIO_PIN_SET
            ? EMELA_GPIO_PIN_STATE_HIGH : EMELA_GPIO_PIN_STATE_LOW;
        emela_gpio_dispatch_interrupt((EmelaGpioPin){.id = (int32_t)(port * 16 + line)}, level);
    }
}

static void dispatch_lines(uint32_t group) {
    uint32_t pending = EXTI->PR & EXTI->IMR & group;
    for (uint32_t line = 0; line < 16; ++line) {
        uint32_t bit = UINT32_C(1) << line;
        if (pending & bit) HAL_GPIO_EXTI_IRQHandler((uint16_t)bit);
    }
    /* Complete the peripheral acknowledgement before exception return. */
    __DSB();
}

void EXTI0_IRQHandler(void) { dispatch_lines(UINT32_C(1) << 0); }
void EXTI1_IRQHandler(void) { dispatch_lines(UINT32_C(1) << 1); }
void EXTI2_IRQHandler(void) { dispatch_lines(UINT32_C(1) << 2); }
void EXTI3_IRQHandler(void) { dispatch_lines(UINT32_C(1) << 3); }
void EXTI4_IRQHandler(void) { dispatch_lines(UINT32_C(1) << 4); }
void EXTI9_5_IRQHandler(void) { dispatch_lines(UINT32_C(0x03e0)); }
void EXTI15_10_IRQHandler(void) { dispatch_lines(UINT32_C(0xfc00)); }
