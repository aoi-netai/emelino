#include <stdint.h>
#include "stm32f4xx_hal.h"

volatile uint32_t uart_test_stage;
volatile uint32_t uart_test_command;
volatile uint32_t uart_test_checks;
volatile uint32_t uart_test_failures;
volatile uint32_t uart_test_last_failure;

void uart_test_check(int32_t number, int32_t success) {
    uart_test_checks++;
    if (!success) {
        uart_test_failures++;
        uart_test_last_failure = (uint32_t)number;
    }
}

int32_t uart_test_now(void) { return (int32_t)HAL_GetTick(); }

void uart_test_wait(int32_t stage) {
    uart_test_stage = (uint32_t)stage;
    while (uart_test_command != (uint32_t)stage) __asm volatile("nop");
}
