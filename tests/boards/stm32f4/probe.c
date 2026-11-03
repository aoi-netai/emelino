#include <stdint.h>

/* Test observations are separate from the production BSP. */
volatile uint32_t gpio_test_stage;
volatile uint32_t gpio_test_command;
volatile uint32_t gpio_test_checks;
volatile uint32_t gpio_test_failures;
volatile uint32_t gpio_test_last_failure;
volatile uint32_t gpio_test_event_count;
volatile uint32_t gpio_test_events[16];
volatile uint32_t gpio_test_ipsr[16];

void gpio_test_check(int32_t number, int32_t success) {
    gpio_test_checks++;
    if (!success) {
        gpio_test_failures++;
        gpio_test_last_failure = (uint32_t)number;
    }
}

void gpio_test_record(int32_t tag, int32_t pin, int32_t level) {
    uint32_t index = gpio_test_event_count;
    uint32_t ipsr;
    __asm volatile("mrs %0, ipsr" : "=r"(ipsr));
    if (index < 16) {
        gpio_test_events[index] = (uint32_t)(tag * 1000 + pin * 10 + level);
        gpio_test_ipsr[index] = ipsr;
    }
    gpio_test_event_count = index + 1;
}

int32_t gpio_test_seed(void) { return 20; }

void gpio_test_wait(int32_t stage) {
    gpio_test_stage = (uint32_t)stage;
    /* The emulator advances this command; real IRQs preempt this thread. */
    while (gpio_test_command != (uint32_t)stage) __asm volatile("nop");
}
