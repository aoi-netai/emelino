/* SPDX-License-Identifier: Apache-2.0 */
#include "bsp.h"
#include "board_config.h"
#include "emela_uart.h"

volatile uint32_t uart_regression_stage, uart_regression_command;
volatile uint32_t uart_regression_checks, uart_regression_failures, uart_regression_last_failure;
static void check(uint32_t number, bool success) {
    ++uart_regression_checks;
    if (!success) { ++uart_regression_failures; uart_regression_last_failure = number; }
}
static void wait_stage(uint32_t stage) {
    uart_regression_stage = stage;
    while (uart_regression_command < stage) __WFI();
}
void uart_regression_failed(void) { check(99, false); }
void uart_regression_run(void) {
    const EmelaUartId id = EMELA_UART_ID_UART0;
    uint16_t words[64], length;
    int32_t discarded;
    check(1, emela_uart_enable_interrupt(id, EMELA_INTERRUPT_PRIORITY_NORMAL, 1) == 0);
    check(2, emela_uart_set_receive_mode(id, EMELA_UART_RECEIVE_MODE_INTERRUPT_RECEIVE) == 0);
    wait_stage(1);
    check(3, emela_uart_receive(id, 64, 10, words, 64, &length) == 0 && length == 64);
    bool all = true;
    for (unsigned i = 0; i < 64; ++i) all &= words[i] == i;
    check(4, all);
    wait_stage(2);
    length = 42;
    check(5, emela_uart_receive(id, 1, 10, words, 64, &length) == EMELA_UART_ERROR_KIND_OVERRUN + 1 && length == 0);
    check(6, emela_uart_discard_input(id, 100, &discarded) == 0 && discarded == 64);
    check(7, emela_uart_discard_input(id, 100, &discarded) == 0 && discarded == 0);
    wait_stage(3);
    check(8, emela_uart_receive_some(id, 64, 10, words, 64, &length) == 0 && length == 2
             && words[0] == 0 && words[1] == 255);
    uint64_t start = nrf52_now_us();
    check(9, emela_uart_receive(id, 1, 2, words, 64, &length) == EMELA_UART_ERROR_KIND_TIMEOUT + 1
             && length == 0 && nrf52_now_us() - start >= 2000 && nrf52_now_us() - start < 3000);
    check(10, emela_uart_receive(id, 65, 10, words, 64, &length) == EMELA_UART_ERROR_KIND_CAPACITY_EXCEEDED + 1);
    check(11, emela_uart_receive(id, 0, 10, words, 64, &length) == EMELA_UART_ERROR_KIND_INVALID_ARGUMENT + 1);
    uint16_t invalid = 256;
    check(12, emela_uart_transmit(id, &invalid, 1, 10) == EMELA_UART_ERROR_KIND_INVALID_ARGUMENT + 1);
    EmelaUartConfig config = {
        .baud_rate = 115200, .data_bits = EMELA_UART_DATA_BITS_BITS8,
        .stop_bits = EMELA_UART_STOP_BITS_TWO, .parity = EMELA_UART_PARITY_EVEN,
        .direction = EMELA_UART_DIRECTION_RECEIVE, .flow_control = EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL,
        .oversampling = EMELA_UART_OVERSAMPLING_NATIVE,
    };
    check(13, emela_uart_configure(id, config) == 0);
    check(14, NRF_UART0->CONFIG == ((UART_CONFIG_PARITY_Included << UART_CONFIG_PARITY_Pos)
              | (UART_CONFIG_STOP_Two << UART_CONFIG_STOP_Pos)));
    check(15, nrf52_pin_owner(EMELA_UART0_TX_PIN) == 0 && nrf52_pin_owner(EMELA_UART0_RX_PIN) == NRF52_OWNER_UART0);
    uint16_t word = 'R';
    check(16, emela_uart_transmit(id, &word, 1, 10) == EMELA_UART_ERROR_KIND_INVALID_STATE + 1);
    config.parity = EMELA_UART_PARITY_ODD;
    check(17, emela_uart_configure(id, config) == EMELA_UART_ERROR_KIND_UNSUPPORTED + 1);
    config.parity = EMELA_UART_PARITY_NO_PARITY;
    config.stop_bits = EMELA_UART_STOP_BITS_ONE;
    config.direction = EMELA_UART_DIRECTION_BOTH;
    check(18, emela_uart_configure(id, config) == 0);
    check(19, emela_uart_transmit(id, &word, 1, 10) == 0);
    check(20, emela_uart_receive(EMELA_UART_ID_UART1, 1, 10, words, 64, &length)
              == EMELA_UART_ERROR_KIND_UNSUPPORTED + 1);
    uart_regression_stage = 4;
}
