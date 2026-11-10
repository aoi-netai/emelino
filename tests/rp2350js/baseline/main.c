/* Plain Pico SDK control image: no emelino-generated C or BSP. */
#include "pico/stdlib.h"
#include "hardware/uart.h"
#include "hardware/gpio.h"
#include "hardware/sync.h"
#include "hardware/i2c.h"

volatile uint32_t pico_baseline_done, pico_baseline_stage;
volatile uint32_t pico_baseline_elapsed, pico_baseline_callbacks;
volatile uint32_t pico_baseline_uart_rsr, pico_baseline_i2c_enable, pico_baseline_i2c_enable_status;
static void on_gpio(uint pin, uint32_t events) {
    if (pin == 3 && (events & GPIO_IRQ_EDGE_RISE)) pico_baseline_callbacks++;
}
int main(void) {
    pico_baseline_stage = 1;
    gpio_init(25);
    gpio_set_dir(25, GPIO_OUT);
    gpio_put(25, 1);
    uart_init(uart0, 115200);
    gpio_set_function(0, GPIO_FUNC_UART);
    gpio_set_function(1, GPIO_FUNC_UART);
    uart_puts(uart0, "SDK\n");
    uart_get_hw(uart0)->rsr = 0;
    pico_baseline_uart_rsr = uart_get_hw(uart0)->rsr;
    i2c_init(i2c0, 100000);
    i2c_get_hw(i2c0)->enable = 0;
    busy_wait_us_32(10);
    pico_baseline_i2c_enable = i2c_get_hw(i2c0)->enable;
    pico_baseline_i2c_enable_status = i2c_get_hw(i2c0)->enable_status;
    pico_baseline_stage = 2;
    uint64_t before = time_us_64();
    busy_wait_us_32(100);
    pico_baseline_elapsed = (uint32_t)(time_us_64() - before);
    gpio_init(3);
    gpio_set_dir(3, GPIO_IN);
    gpio_set_irq_enabled_with_callback(3, GPIO_IRQ_EDGE_RISE, true, on_gpio);
    pico_baseline_stage = 3;
    uint64_t deadline = time_us_64() + 1000;
    while (!pico_baseline_callbacks && time_us_64() < deadline) tight_loop_contents();
    pico_baseline_done = 1;
    for (;;) __wfi();
}
