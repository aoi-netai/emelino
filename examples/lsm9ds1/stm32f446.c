#include <stdint.h>
#include "bsp.h"

/* Main-context delay; the peripheral firmware starts SysTick. */
void lsm9ds1_delay_ms(int32_t ms) {
    if (ms <= 0) return;
    uint32_t started = HAL_GetTick();
    while ((uint32_t)(HAL_GetTick() - started) < (uint32_t)ms) __WFI();
}
