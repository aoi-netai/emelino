/* SPDX-License-Identifier: Apache-2.0 */
#include <limits.h>
#include "bsp.h"
#include "esp_rom_sys.h"
#include "esp_timer.h"

uint64_t emelino_idf_now_us(void) { return (uint64_t)esp_timer_get_time(); }
bool emelino_idf_timeout_valid(int64_t timeout) { return timeout > 0 && timeout <= INT32_MAX; }
int emelino_idf_remaining_ms(uint64_t deadline) {
    uint64_t now = emelino_idf_now_us();
    return now >= deadline ? 0 : (int)((deadline - now) / 1000);
}
void emelino_idf_poll_wait(uint64_t deadline) {
    uint64_t now = emelino_idf_now_us();
    if (now < deadline && deadline - now > 1000) vTaskDelay(1);
    else taskYIELD();
}
bool emelino_idf_delay_us(uint64_t delay, uint64_t deadline) {
    uint64_t start = emelino_idf_now_us();
    if (start >= deadline || delay > deadline - start) return false;
    while (emelino_idf_now_us() - start < delay) {
        uint64_t now = emelino_idf_now_us();
        if (now >= deadline) return false;
        if (now - start >= delay) break;
        if (delay - (now - start) > 2000) vTaskDelay(1);
        else esp_rom_delay_us(1);
    }
    return emelino_idf_now_us() < deadline;
}
