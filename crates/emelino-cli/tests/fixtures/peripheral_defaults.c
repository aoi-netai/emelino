/* Observe the existing BSP ABI: shorthand must supply precisely these values. */
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
#include "emela_gpio.h"
#include "emela_uart.h"
#include "emela_i2c.h"
#include "emela_spi.h"

static unsigned calls;
static unsigned arguments;
#ifndef EXPECT_ARGUMENTS
#define EXPECT_ARGUMENTS 0
#endif
#ifndef UART_TRANSMIT_TIMEOUT
#define UART_TRANSMIT_TIMEOUT 1000
#endif
#ifndef UART_PRIORITY
#define UART_PRIORITY EMELA_INTERRUPT_PRIORITY_NORMAL
#endif
#ifndef UART_NOTIFY
#define UART_NOTIFY 0
#endif
#ifndef I2C_READY_TRIALS
#define I2C_READY_TRIALS 1
#endif
#ifndef SPI_TRANSACTION_TIMEOUT
#define SPI_TRANSACTION_TIMEOUT 1000
#endif
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
    (void)kind; (void)message; (void)length; abort();
}
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t previous) { assert(previous == 0); }

int32_t emela_gpio_configure(EmelaGpioPin pin, EmelaGpioConfig config) {
    ++calls;
    assert(pin.id == 0 && config.tag == EMELA_GPIO_CONFIG_INPUT);
    assert(config.payload.input.pull == EMELA_GPIO_PULL_NO_PULL);
    return 0;
}
int32_t emela_gpio_enable_interrupt(EmelaGpioPin pin, EmelaGpioEdge edge, EmelaInterruptPriority priority) {
    ++calls;
    assert(pin.id == 0 && edge == EMELA_GPIO_EDGE_RISING && priority == EMELA_INTERRUPT_PRIORITY_NORMAL);
    return 0;
}
int32_t emela_gpio_disable_interrupt(EmelaGpioPin pin) { ++calls; assert(pin.id == 0); return 0; }

int32_t emela_uart_configure(EmelaUartId id, EmelaUartConfig config) {
    ++calls;
    assert(id == EMELA_UART_ID_UART2 && config.baud_rate == 115200);
    assert(config.data_bits == EMELA_UART_DATA_BITS_BITS8 && config.stop_bits == EMELA_UART_STOP_BITS_ONE);
    assert(config.parity == EMELA_UART_PARITY_NO_PARITY && config.direction == EMELA_UART_DIRECTION_BOTH);
    assert(config.flow_control == EMELA_UART_FLOW_CONTROL_NO_FLOW_CONTROL && config.oversampling == EMELA_UART_OVERSAMPLING_NATIVE);
    return 0;
}
int32_t emela_uart_transmit(EmelaUartId id, const uint16_t *data, uint16_t length, int64_t timeout) {
    ++calls; assert(id == EMELA_UART_ID_UART2 && length == 1 && data[0] == 42 && timeout == UART_TRANSMIT_TIMEOUT); return 0;
}
int32_t emela_uart_receive(EmelaUartId id, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    ++calls; assert(id == EMELA_UART_ID_UART2 && size == 1 && timeout == 1000 && capacity >= 1);
    out[0] = 42; *length = 1; return 0;
}
int32_t emela_uart_receive_some(EmelaUartId id, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    ++calls; assert(id == EMELA_UART_ID_UART2 && size == 2 && timeout == 1000 && capacity >= 2);
    out[0] = 42; *length = 1; return 0;
}
int32_t emela_uart_enable_interrupt(EmelaUartId id, EmelaInterruptPriority priority, int32_t notify) {
    ++calls; assert(id == EMELA_UART_ID_UART2 && priority == UART_PRIORITY && notify == UART_NOTIFY); return 0;
}

int32_t emela_i2c_configure(EmelaI2cId id, EmelaI2cConfig config) {
    ++calls; assert(id == EMELA_I2C_ID_I2C1 && config.clock_speed_hz == 100000 && config.duty_cycle == EMELA_I2C_DUTY_CYCLE_STANDARD); return 0;
}
int32_t emela_i2c_master_transmit(EmelaI2cId id, int32_t address, const uint16_t *data, uint16_t length, int64_t timeout) {
    ++calls; assert(id == EMELA_I2C_ID_I2C1 && address == 80 && length == 1 && data[0] == 42 && timeout == 1000); return 0;
}
int32_t emela_i2c_write_read(EmelaI2cId id, int32_t address, const uint16_t *data, uint16_t count, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    (void)data; ++calls;
    assert(id == EMELA_I2C_ID_I2C1 && address == 80 && count == 0 && size == 1 && timeout == 1000 && capacity >= 1);
    out[0] = 42; *length = 1; return 0;
}
int32_t emela_i2c_memory_write(EmelaI2cId id, int32_t address, int32_t memory_address, EmelaI2cMemoryAddressSize address_size, const uint16_t *data, uint16_t length, int64_t timeout) {
    ++calls;
    assert(id == EMELA_I2C_ID_I2C1 && address == 80 && memory_address == 3 && address_size == EMELA_I2C_MEMORY_ADDRESS_SIZE_BITS8);
    assert(length == 1 && data[0] == 42 && timeout == 1000); return 0;
}
int32_t emela_i2c_is_device_ready(EmelaI2cId id, int32_t address, int32_t trials, int64_t timeout, int32_t *ready) {
    ++calls; assert(id == EMELA_I2C_ID_I2C1 && address == 80 && trials == I2C_READY_TRIALS && timeout == 1000); *ready = 1; return 0;
}
int32_t emela_i2c_recover(EmelaI2cId id, int64_t timeout) {
    ++calls; assert(id == EMELA_I2C_ID_I2C1 && timeout == 1000); return 0;
}
int32_t emela_i2c_enable_interrupt(EmelaI2cId id, EmelaInterruptPriority priority) {
    ++calls; assert(id == EMELA_I2C_ID_I2C1 && priority == EMELA_INTERRUPT_PRIORITY_NORMAL); return 0;
}

static void spi_config(EmelaSpiConfig config) {
    assert(config.mode == EMELA_SPI_MODE_MASTER && config.direction == EMELA_SPI_DIRECTION_TWO_LINE && config.data_size == EMELA_SPI_DATA_SIZE_BITS8);
    assert(config.polarity == EMELA_SPI_CLOCK_POLARITY_IDLE_LOW && config.phase == EMELA_SPI_CLOCK_PHASE_FIRST_EDGE);
    assert(config.max_frequency_hz == 4000000 && config.bit_order == EMELA_SPI_BIT_ORDER_MOST_SIGNIFICANT_FIRST);
}
int32_t emela_spi_configure(EmelaSpiId id, EmelaSpiConfig config) {
    ++calls; assert(id == EMELA_SPI_ID_SPI1); spi_config(config); return 0;
}
int32_t emela_spi_transmit(EmelaSpiId id, const uint16_t *data, uint16_t length, int64_t timeout) {
    ++calls; assert(id == EMELA_SPI_ID_SPI1 && length == 1 && data[0] == 42 && timeout == 1000); return 0;
}
int32_t emela_spi_receive(EmelaSpiId id, int32_t size, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    ++calls; assert(id == EMELA_SPI_ID_SPI1 && size == 1 && timeout == 1000 && capacity >= 1);
    out[0] = 42; *length = 1; return 0;
}
int32_t emela_spi_transfer(EmelaSpiId id, const uint16_t *data, uint16_t count, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *length) {
    assert(count == 1 && data[0] == 42); return emela_spi_receive(id, 1, timeout, out, capacity, length);
}
EmelaSpiTransactionStatus emela_spi_create_device(EmelaSpiDeviceConfig config, EmelaSpiDevice *out) {
    ++calls; assert(config.bus == EMELA_SPI_ID_SPI1 && config.cs.id == 4 && config.active == EMELA_GPIO_PIN_STATE_LOW && config.setup_us == 0 && config.hold_us == 0);
    spi_config(config.config); out->slot = 7; return (EmelaSpiTransactionStatus){0};
}
EmelaSpiTransactionStatus emela_spi_transaction(EmelaSpiDevice device, const EmelaSpiOperation *operations, uint16_t count, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *lengths) {
    ++calls; assert(device.slot == 7 && count == 1 && operations[0].tag == EMELA_SPI_OPERATION_READ && operations[0].length == 1 && timeout == SPI_TRANSACTION_TIMEOUT && capacity >= 1);
    out[0] = 42; lengths[0] = 1; return (EmelaSpiTransactionStatus){0};
}

/* Board handlers retain their complete interfaces. These operations are unused. */
#define UNEXPECTED(signature) signature { abort(); }
UNEXPECTED(int32_t emela_gpio_read(EmelaGpioPin p, EmelaGpioPinState *out))
UNEXPECTED(int32_t emela_gpio_write(EmelaGpioPin p, EmelaGpioPinState s))
UNEXPECTED(int32_t emela_gpio_toggle(EmelaGpioPin p))
UNEXPECTED(int32_t emela_uart_discard_input(EmelaUartId id, int32_t size, int32_t *out))
UNEXPECTED(int32_t emela_uart_set_receive_mode(EmelaUartId id, EmelaUartReceiveMode mode))
UNEXPECTED(int32_t emela_uart_disable_interrupt(EmelaUartId id))
UNEXPECTED(int32_t emela_i2c_disable_interrupt(EmelaI2cId id))
UNEXPECTED(EmelaSpiTransactionStatus emela_spi_recover(EmelaSpiId id))

int main(void) { assert(entry() == 42); assert(calls == 21); assert(arguments == EXPECT_ARGUMENTS); return 0; }
