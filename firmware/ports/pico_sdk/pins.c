/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"

static uint16_t owners[32];

bool emelino_pico_pin_valid(int32_t pin) {
    return pin >= 0 && pin < 32 && (EMELA_GPIO_PIN_MASK & (UINT32_C(1) << pin));
}

uint16_t emelino_pico_pin_owner(int32_t pin) {
    if (!emelino_pico_pin_valid(pin)) return 0;
    uint32_t previous = emelino_pico_interrupt_lock();
    uint16_t owner = owners[pin];
    emelino_pico_interrupt_unlock(previous);
    return owner;
}

int32_t emelino_pico_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (!owner || (count && !pins)) return 1;
    uint32_t previous = emelino_pico_interrupt_lock();
    int32_t status = 0;
    for (uint16_t i = 0; i < count; ++i) {
        if (!emelino_pico_pin_valid(pins[i])) { status = 1; break; }
        if (owners[pins[i]] && owners[pins[i]] != owner) { status = 5; break; }
    }
    /* Validate the whole set first: a failed claim never acquires a partial bus. */
    if (!status)
        for (uint16_t i = 0; i < count; ++i) owners[pins[i]] = owner;
    emelino_pico_interrupt_unlock(previous);
    return status;
}

void emelino_pico_pins_release(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (!owner || (count && !pins)) return;
    uint32_t previous = emelino_pico_interrupt_lock();
    for (uint16_t i = 0; i < count; ++i)
        if (emelino_pico_pin_valid(pins[i]) && owners[pins[i]] == owner) owners[pins[i]] = 0;
    emelino_pico_interrupt_unlock(previous);
}
