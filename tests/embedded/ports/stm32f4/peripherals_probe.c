#include "bsp.h"

volatile uint32_t peripherals_test_checks;
volatile uint32_t peripherals_test_failures;
volatile uint32_t peripherals_test_last_failure;

void peripherals_test_check(int32_t number, int32_t success) {
    ++peripherals_test_checks;
    if (!success) {
        ++peripherals_test_failures;
        peripherals_test_last_failure = (uint32_t)number;
    }
}

int32_t peripherals_test_claim_rollback(void) {
    const int32_t existing = 9;
    const int32_t pins[] = {8, 9};
    if (stm32f4_pins_claim(STM32F4_OWNER_GPIO, &existing, 1)) return 0;
    int32_t status = stm32f4_pins_claim(STM32F4_OWNER_USART2, pins, 2);
    bool valid = status == 5 && stm32f4_pin_owner(8) == 0 &&
                 stm32f4_pin_owner(9) == STM32F4_OWNER_GPIO;
    stm32f4_pins_release(STM32F4_OWNER_GPIO, &existing, 1);
    return valid;
}
