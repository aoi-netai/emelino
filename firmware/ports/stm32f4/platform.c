/* SPDX-License-Identifier: Apache-2.0 */
#include <stdint.h>

#include "emela_board.h"
#include "emela_embedded.h"
#include "emela_embedded_exports.h"
#include "registers.h"
#include "board_config.h"
#include "bsp.h"

/* Debugger/Renode observations; normal firmware does not depend on them. */
volatile uint32_t emela_board_completed;
volatile uint32_t emela_board_failures;
volatile uint32_t emela_board_last_defect;
volatile uint32_t emela_board_boot_data = UINT32_C(0x454d4244);
volatile uint32_t emela_board_boot_bss;
volatile uint32_t emela_board_boot_initialized;

void _init(void) {}
void _fini(void) {}

/* A board may provide init.c for additional board-level setup. */
__attribute__((weak)) void emela_board_init(void) {}

_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
    (void)message;
    (void)length;
    __disable_irq();
    emela_board_last_defect = (uint32_t)kind;
    emela_board_failures++;
    for (;;) __WFI();
}

uint32_t emela_board_event_lock(void) {
    return stm32f4_interrupt_lock();
}

void emela_board_event_unlock(uint32_t previous) {
    stm32f4_interrupt_unlock(previous);
}

void emela_board_idle(void) {
    uint32_t previous = stm32f4_interrupt_lock();
#ifdef EMELA_HAS_TASK_RUNTIME
    if (!emela_scheduler_ready()) {
#endif
        __DSB();
        __WFI();
#ifdef EMELA_HAS_TASK_RUNTIME
    }
#endif
    stm32f4_interrupt_unlock(previous);
}

/* Debug output needs a future Console driver; GPIO-only firmware is silent. */
void emela_board_debug(const char *bytes, uint16_t length) {
    (void)bytes;
    (void)length;
}

void emela_platform_start(void (*program)(void)) {
    program();
    emela_board_completed = 1;
    /* GPIO subscriptions remain active after the setup function returns. */
    for (;;) emela_board_idle();
}

int main(void) {
    if (emela_board_boot_data != UINT32_C(0x454d4244) || emela_board_boot_bss != 0)
        emela_board_defect(EMELA_DEFECT_ASSERT, "reset initialization", 20);
    emela_board_boot_bss = UINT32_C(0x4253534f);
    emela_board_boot_initialized = 1;
#ifdef EMELA_PERIPHERAL_TIME
    stm32f4_time_init();
#endif
    emela_board_init();
    emela_embedded_start();
    for (;;) emela_board_idle();
}
