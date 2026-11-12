/* SPDX-License-Identifier: Apache-2.0 */
#include <stddef.h>
#include <string.h>
#include "board_config.h"
#include "bsp.h"
#include "emela_board.h"
#include "emela_embedded.h"
#include "emela_embedded_exports.h"
#include "esp_rom_sys.h"

volatile uint32_t emela_board_completed, emela_board_failures, emela_board_last_defect;
volatile uint32_t emela_board_boot_data = UINT32_C(0x454d4244);
volatile uint32_t emela_board_boot_bss, emela_board_boot_initialized;
/* Stack grows down into the reserved guard. Both ranges are visible in ELF. */
static struct {
    uint8_t guard[EMELA_STACK_GUARD_BYTES];
    StackType_t stack[EMELA_STACK_BYTES / sizeof(StackType_t)];
} __attribute__((aligned(16))) application_storage;
static StaticTask_t application_task_control;

__attribute__((weak)) void emela_board_init(void) {}
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
    (void)message; (void)length;
    emela_board_last_defect = (uint32_t)kind;
    ++emela_board_failures;
    /* Also callable from an ISR; do not call task APIs here. */
    portDISABLE_INTERRUPTS();
    for (;;) esp_rom_delay_us(1000);
}
uint32_t emela_board_event_lock(void) { return emelino_idf_lock(); }
void emela_board_event_unlock(uint32_t previous) { emelino_idf_unlock(previous); }
void emela_board_idle(void) {
    for (size_t i = 0; i < sizeof(application_storage.guard); ++i)
        if (application_storage.guard[i] != 0xa5)
            emela_board_defect(EMELA_DEFECT_ASSERT, "stack guard", 11);
    /* Yield to IDF/idle tasks; pending Emela events are checked next tick. */
    vTaskDelay(1);
}
void emela_board_debug(const char *bytes, uint16_t length) { (void)bytes; (void)length; }
void emela_platform_start(void (*program)(void)) {
    program();
    emela_board_completed = 1;
    for (;;) emela_board_idle();
}
static void application_task(void *argument) {
    (void)argument;
    emela_board_init();
    emela_embedded_start();
    for (;;) emela_board_idle();
}
void app_main(void) {
    if (emela_board_boot_data != UINT32_C(0x454d4244) || emela_board_boot_bss)
        emela_board_defect(EMELA_DEFECT_ASSERT, "reset initialization", 20);
    emela_board_boot_bss = UINT32_C(0x4253534f);
    emela_board_boot_initialized = 1;
    memset(application_storage.guard, 0xa5, sizeof(application_storage.guard));
    /* ESP-IDF specifies stack depth in bytes, including for static tasks. */
    if (!xTaskCreateStatic(application_task, "emelino", sizeof(application_storage.stack), NULL,
                           5, application_storage.stack, &application_task_control))
        emela_board_defect(EMELA_DEFECT_ASSERT, "application task", 16);
}
