/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"

static const uint32_t available[EMELA_GPIO_PORT_COUNT] = EMELA_GPIO_PIN_MASKS;
static uint16_t owners[EMELA_GPIO_PORT_COUNT * 32];

NRF_GPIO_Type *nrf52_gpio_port(uint32_t port) { return port == 0 ? NRF_P0 : NRF_P1; }

bool nrf52_pin_valid(int32_t pin) {
    return pin >= 0 && (uint32_t)pin < EMELA_GPIO_PORT_COUNT * 32
        && (available[(uint32_t)pin / 32] & (UINT32_C(1) << ((uint32_t)pin % 32)));
}

uint16_t nrf52_pin_owner(int32_t pin) {
    if (!nrf52_pin_valid(pin)) return 0;
    uint32_t previous = nrf52_interrupt_lock();
    uint16_t owner = owners[pin];
    nrf52_interrupt_unlock(previous);
    return owner;
}

int32_t nrf52_pins_claim(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (!owner || (count && !pins)) return 1;
    uint32_t previous = nrf52_interrupt_lock();
    int32_t result = 0;
    for (uint16_t i = 0; i < count; ++i) {
        if (!nrf52_pin_valid(pins[i])) { result = 1; break; }
        if (owners[pins[i]] && owners[pins[i]] != owner) { result = 5; break; }
    }
    if (!result)
        for (uint16_t i = 0; i < count; ++i) owners[pins[i]] = owner;
    nrf52_interrupt_unlock(previous);
    return result;
}

void nrf52_pins_release(uint16_t owner, const int32_t *pins, uint16_t count) {
    if (!owner || (count && !pins)) return;
    uint32_t previous = nrf52_interrupt_lock();
    for (uint16_t i = 0; i < count; ++i)
        if (nrf52_pin_valid(pins[i]) && owners[pins[i]] == owner) owners[pins[i]] = 0;
    nrf52_interrupt_unlock(previous);
}
