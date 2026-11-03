/* Real BSP calls: the host drops controller completion events, never responses. */
#include "bsp.h"
#include "emela_spi.h"
#include "emela_i2c.h"

volatile uint32_t recovery_stage, recovery_command;
volatile uint32_t recovery_checks, recovery_failures, recovery_last_failure;
volatile int32_t recovery_spi_timeout, recovery_spi_blocked;
volatile int32_t recovery_spi_recover_status, recovery_spi_read_status;
volatile int32_t recovery_twi_poll_timeout, recovery_twi_irq_timeout;
volatile uint32_t recovery_spi_elapsed, recovery_twi_poll_elapsed, recovery_twi_irq_elapsed;
volatile uint32_t recovery_jedec, recovery_poll_sensor_id, recovery_irq_sensor_id;

static void check(uint32_t number, bool okay) {
    recovery_checks++;
    if (!okay) { recovery_failures++; recovery_last_failure = number; }
}
void recovery_regression_failed(void) { check(100, false); }
static void wait_stage(uint32_t stage) {
    recovery_stage = stage;
    while (recovery_command < stage) __WFI();
}
static bool status_is(EmelaSpiTransactionStatus status, int32_t expected) {
    return status.primary_spi == expected && !status.primary_gpio && !status.cleanup_gpio;
}
static EmelaSpiTransactionStatus identify(EmelaSpiDevice flash, int64_t timeout, uint16_t *result) {
    const uint16_t command = 0x9f;
    const EmelaSpiOperation operations[] = {
        {EMELA_SPI_OPERATION_WRITE, &command, 1}, {EMELA_SPI_OPERATION_READ, 0, 3},
    };
    uint16_t lengths[2];
    return emela_spi_transaction(flash, operations, 2, timeout, result, 3, lengths);
}
static int32_t sensor_id(int64_t timeout, uint16_t *result, uint16_t *length) {
    const uint16_t address = 0xd0;
    return emela_i2c_write_read(EMELA_I2C_ID_I2C1, 0x77, &address, 1, 1,
                               timeout, result, 1, length);
}
void recovery_regression_run(void) {
    const EmelaSpiDeviceConfig config = {
        .bus = EMELA_SPI_ID_SPI1,
        .config = {EMELA_SPI_MODE_MASTER, EMELA_SPI_DIRECTION_TWO_LINE,
                   EMELA_SPI_DATA_SIZE_BITS8, EMELA_SPI_CLOCK_POLARITY_IDLE_LOW,
                   EMELA_SPI_CLOCK_PHASE_FIRST_EDGE, 1000000,
                   EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST},
        .cs = {34}, .active = EMELA_GPIO_PIN_STATE_LOW, .setup_us = 1, .hold_us = 1,
    };
    EmelaSpiDevice flash;
    check(1, status_is(emela_spi_create_device(config, &flash), 0));
    uint16_t result[3] = {0}, length;
    wait_stage(1);
    uint64_t start = nrf52_now_us();
    EmelaSpiTransactionStatus status = identify(flash, 2, result);
    recovery_spi_elapsed = (uint32_t)(nrf52_now_us() - start);
    recovery_spi_timeout = status.primary_spi;
    check(2, status_is(status, 5));
    check(3, recovery_spi_elapsed >= 2000 && recovery_spi_elapsed < 10000);
    check(4, (NRF_P1->OUT & (1u << 2)) != 0);
    status = identify(flash, 20, result);
    recovery_spi_blocked = status.primary_spi;
    check(5, status_is(status, 8));
    wait_stage(2);
    status = emela_spi_recover(EMELA_SPI_ID_SPI1);
    recovery_spi_recover_status = status.primary_spi;
    check(6, status_is(status, 0));
    status = identify(flash, 20, result);
    recovery_spi_read_status = status.primary_spi;
    recovery_jedec = (uint32_t)result[0] << 16 | (uint32_t)result[1] << 8 | result[2];
    check(7, status_is(status, 0));
    check(8, result[0] == 0x20 && result[1] == 0xbb && result[2] == 0x17);

    wait_stage(3);
    start = nrf52_now_us();
    recovery_twi_poll_timeout = sensor_id(2, result, &length);
    recovery_twi_poll_elapsed = (uint32_t)(nrf52_now_us() - start);
    check(9, recovery_twi_poll_timeout == 5);
    check(10, recovery_twi_poll_elapsed >= 2000 && recovery_twi_poll_elapsed < 10000);
    check(11, length == 0);
    wait_stage(4);
    check(12, emela_i2c_recover(EMELA_I2C_ID_I2C1, 20) == 0);
    check(13, sensor_id(20, result, &length) == 0);
    recovery_poll_sensor_id = result[0];
    check(14, length == 1 && result[0] == 0x55);
    check(15, emela_i2c_enable_interrupt(EMELA_I2C_ID_I2C1, EMELA_INTERRUPT_PRIORITY_NORMAL) == 0);
    wait_stage(5);
    start = nrf52_now_us();
    recovery_twi_irq_timeout = sensor_id(2, result, &length);
    recovery_twi_irq_elapsed = (uint32_t)(nrf52_now_us() - start);
    check(16, recovery_twi_irq_timeout == 5);
    check(17, recovery_twi_irq_elapsed >= 2000 && recovery_twi_irq_elapsed < 10000);
    check(18, length == 0);
    wait_stage(6);
    check(19, emela_i2c_recover(EMELA_I2C_ID_I2C1, 20) == 0);
    check(20, sensor_id(20, result, &length) == 0);
    recovery_irq_sensor_id = result[0];
    check(21, length == 1 && result[0] == 0x55);
    check(22, emela_i2c_disable_interrupt(EMELA_I2C_ID_I2C1) == 0);
    recovery_stage = 7;
}
