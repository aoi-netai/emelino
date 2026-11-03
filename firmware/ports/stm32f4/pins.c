/* SPDX-License-Identifier: Apache-2.0 */
#include "board_config.h"
#include "bsp.h"

static const uint16_t package_pins[EMELA_GPIO_PORT_COUNT] = EMELA_GPIO_PIN_MASKS;
static uint16_t owners[EMELA_GPIO_PORT_COUNT * 16];

bool stm32f4_pin_valid(int32_t pin) {
    return pin >= 0 && (uint32_t)pin < EMELA_GPIO_PORT_COUNT * 16
        && (package_pins[(uint32_t)pin / 16] & (1U << ((uint32_t)pin % 16)));
}

uint16_t stm32f4_pin_owner(int32_t pin) {
    if (!stm32f4_pin_valid(pin)) return 0;
    uint32_t previous = stm32f4_interrupt_lock();
    uint16_t owner = owners[pin];
    stm32f4_interrupt_unlock(previous);
    return owner;
}

int32_t stm32f4_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (owner == 0 || (count != 0 && pins == 0)) return 1;
    uint32_t previous = stm32f4_interrupt_lock();
    int32_t result = 0;
    for (uint16_t i = 0; i < count; ++i) {
        if (!stm32f4_pin_valid(pins[i])) { result = 1; break; }
        if (owners[pins[i]] != 0 && owners[pins[i]] != owner) { result = 5; break; }
    }
    if (result == 0)
        for (uint16_t i = 0; i < count; ++i) owners[pins[i]] = owner;
    stm32f4_interrupt_unlock(previous);
    return result;
}

void stm32f4_pins_release(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (owner == 0 || (count != 0 && pins == 0)) return;
    uint32_t previous = stm32f4_interrupt_lock();
    for (uint16_t i = 0; i < count; ++i)
        if (stm32f4_pin_valid(pins[i]) && owners[pins[i]] == owner) owners[pins[i]] = 0;
    stm32f4_interrupt_unlock(previous);
}
