#include "../../support/probe.h"
#include "emela_gpio.h"
#include "emela_uart.h"
#include "emela_spi.h"
#include "emela_i2c.h"
#include "test_target.h"
EMELINO_TEST_PROBE(contract)
volatile uint32_t contract_isr_calls;
volatile uint32_t contract_isr_ipsr;
volatile uint32_t contract_context_failures;

/* Deliberate ABI calls from forbidden contexts test the hardware port guards.
   They do not route ordinary device transfers or inject device responses. */
static int32_t context_check(void) {
    uint16_t word = 0;
    int32_t okay = emela_uart_transmit(EMELINO_TEST_UART_ID, &word, 1, 1) == 8;
    okay &= emela_spi_transmit(EMELINO_TEST_SPI_ID, &word, 1, 1) == 8;
    okay &= emela_i2c_master_transmit(EMELINO_TEST_I2C_ID, 119, &word, 1, 1) == 8;
    return okay;
}
int32_t contract_masked_check(void) {
    uint32_t previous;
    __asm volatile("mrs %0, primask\ncpsid i" : "=r"(previous) : : "memory");
    int32_t okay = context_check();
    __asm volatile("msr primask, %0" : : "r"(previous) : "memory");
    return okay;
}
void contract_in_isr(void) {
    __asm volatile("mrs %0, ipsr" : "=r"(contract_isr_ipsr));
    contract_isr_calls++;
    if (!context_check()) contract_context_failures++;
}
