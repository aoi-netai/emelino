/* Test double for SDK calls. This does not emulate an ESP32-C3. */
#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
typedef int esp_err_t;
enum { ESP_OK, ESP_ERR_TIMEOUT, ESP_ERR_INVALID_RESPONSE, ESP_ERR_NOT_FOUND, ESP_ERR_NO_MEM,
       ESP_ERR_INVALID_ARG, ESP_ERR_INVALID_STATE, ESP_FAIL };
enum { ESP_INTR_FLAG_LEVEL1 = 1, ESP_INTR_FLAG_LEVEL2 = 2, ESP_INTR_FLAG_LEVEL3 = 4 };
extern bool mock_isr;
uint32_t mock_lock(void);
void mock_unlock(uint32_t previous);
#define portSET_INTERRUPT_MASK_FROM_ISR() mock_lock()
#define portCLEAR_INTERRUPT_MASK_FROM_ISR(previous) mock_unlock(previous)
#define xPortInIsrContext() mock_isr
void taskYIELD(void);
void vTaskDelay(unsigned ticks);
int64_t esp_timer_get_time(void);
void esp_rom_delay_us(unsigned delay);

typedef int gpio_mode_t;
typedef int gpio_int_type_t;
enum { GPIO_MODE_DISABLE = 0, GPIO_MODE_INPUT = 1, GPIO_MODE_DEF_OUTPUT = 2,
       GPIO_MODE_INPUT_OUTPUT = 3, GPIO_MODE_INPUT_OUTPUT_OD = 7,
       GPIO_INTR_DISABLE = 0, GPIO_INTR_POSEDGE, GPIO_INTR_NEGEDGE, GPIO_INTR_ANYEDGE };
typedef struct { uint64_t pin_bit_mask; gpio_mode_t mode; int pull_up_en, pull_down_en; gpio_int_type_t intr_type; } gpio_config_t;
esp_err_t gpio_set_level(int pin, int level);
esp_err_t gpio_config(const gpio_config_t *config);
esp_err_t gpio_reset_pin(int pin);
int gpio_get_level(int pin);
esp_err_t gpio_install_isr_service(int flags);
void gpio_uninstall_isr_service(void);
esp_err_t gpio_intr_disable(int pin);
esp_err_t gpio_intr_enable(int pin);
esp_err_t gpio_set_intr_type(int pin, gpio_int_type_t type);
esp_err_t gpio_isr_handler_add(int pin, void (*handler)(void *), void *argument);
esp_err_t gpio_isr_handler_remove(int pin);

enum { UART_NUM_0, UART_DATA_7_BITS, UART_DATA_8_BITS, UART_PARITY_EVEN, UART_PARITY_ODD,
       UART_PARITY_DISABLE, UART_STOP_BITS_2, UART_STOP_BITS_1, UART_HW_FLOWCTRL_DISABLE,
       UART_SCLK_XTAL, ETS_UART0_INTR_SOURCE };
#define UART_PIN_NO_CHANGE (-1)
typedef struct { int baud_rate, data_bits, parity, stop_bits, flow_ctrl, source_clk; } uart_config_t;
typedef void *intr_handle_t;
esp_err_t esp_intr_alloc(int source, int flags, void (*handler)(void *), void *argument, intr_handle_t *handle);
esp_err_t esp_intr_free(intr_handle_t handle);
bool uart_is_driver_installed(int port);
esp_err_t uart_param_config(int port, const uart_config_t *config);
esp_err_t uart_set_pin(int port, int tx, int rx, int rts, int cts);
typedef struct { uint32_t raw, enabled; unsigned rx_length; uint8_t rx[256]; } uart_dev_t;
extern uart_dev_t mock_uart;
#define UART_LL_GET_HW(number) (&mock_uart)
#define UART_LL_FIFO_DEF_LEN 128
enum { UART_INTR_RXFIFO_FULL = 1, UART_INTR_TXFIFO_EMPTY = 2, UART_INTR_PARITY_ERR = 4,
       UART_INTR_FRAM_ERR = 8, UART_INTR_RXFIFO_OVF = 16, UART_INTR_BRK_DET = 128,
       UART_INTR_RXFIFO_TOUT = 256, UART_INTR_TX_DONE = 16384 };
void uart_ll_disable_intr_mask(uart_dev_t *hw, uint32_t mask);
void uart_ll_ena_intr_mask(uart_dev_t *hw, uint32_t mask);
void uart_ll_clr_intsts_mask(uart_dev_t *hw, uint32_t mask);
uint32_t uart_ll_get_intsts_mask(uart_dev_t *hw);
uint32_t uart_ll_get_intraw_mask(uart_dev_t *hw);
unsigned uart_ll_get_rxfifo_len(uart_dev_t *hw);
unsigned uart_ll_get_txfifo_len(uart_dev_t *hw);
void uart_ll_read_rxfifo(uart_dev_t *hw, uint8_t *out, uint32_t length);
void uart_ll_write_txfifo(uart_dev_t *hw, const uint8_t *data, uint32_t length);
void uart_ll_rxfifo_rst(uart_dev_t *hw);
void uart_ll_txfifo_rst(uart_dev_t *hw);
void uart_ll_set_rxfifo_full_thr(uart_dev_t *hw, unsigned threshold);
void uart_ll_set_rx_tout(uart_dev_t *hw, unsigned timeout);
bool uart_ll_is_tx_idle(uart_dev_t *hw);

enum { SPI2_HOST = 1, SPI_DMA_DISABLED = 0, SPI_DEVICE_BIT_LSBFIRST = 3,
       SPI_TRANS_USE_TXDATA = 1, SPI_TRANS_USE_RXDATA = 2 };
typedef void *spi_device_handle_t;
typedef struct { int mosi_io_num, miso_io_num, sclk_io_num, quadwp_io_num, quadhd_io_num, max_transfer_sz; } spi_bus_config_t;
typedef struct { int clock_speed_hz, mode, spics_io_num, queue_size, flags; } spi_device_interface_config_t;
typedef struct { int flags; size_t length, rxlength; uint8_t tx_data[4], rx_data[4]; } spi_transaction_t;
int spi_ll_master_cal_clock(int clock, int requested, int duty, void *result);
esp_err_t spi_bus_initialize(int host, const spi_bus_config_t *config, int dma);
esp_err_t spi_bus_free(int host);
esp_err_t spi_bus_remove_device(spi_device_handle_t device);
esp_err_t spi_bus_add_device(int host, const spi_device_interface_config_t *config, spi_device_handle_t *device);
esp_err_t spi_device_queue_trans(spi_device_handle_t device, spi_transaction_t *transaction, unsigned timeout);
esp_err_t spi_device_get_trans_result(spi_device_handle_t device, spi_transaction_t **transaction, unsigned timeout);

enum { I2C_NUM_0, I2C_CLK_SRC_DEFAULT, I2C_ADDR_BIT_LEN_7 };
typedef void *i2c_master_bus_handle_t;
typedef void *i2c_master_dev_handle_t;
typedef struct { int i2c_port, scl_io_num, sda_io_num, clk_source, glitch_ignore_cnt;
    struct { bool enable_internal_pullup; } flags; } i2c_master_bus_config_t;
typedef struct { int dev_addr_length; uint16_t device_address; uint32_t scl_speed_hz; } i2c_device_config_t;
esp_err_t i2c_new_master_bus(const i2c_master_bus_config_t *config, i2c_master_bus_handle_t *bus);
esp_err_t i2c_master_probe(i2c_master_bus_handle_t bus, uint16_t address, int timeout);
esp_err_t i2c_master_bus_add_device(i2c_master_bus_handle_t bus, const i2c_device_config_t *config, i2c_master_dev_handle_t *device);
esp_err_t i2c_master_bus_rm_device(i2c_master_dev_handle_t device);
esp_err_t i2c_master_transmit_receive(i2c_master_dev_handle_t device, const uint8_t *tx, size_t tx_length, uint8_t *rx, size_t rx_length, int timeout);
esp_err_t i2c_master_transmit(i2c_master_dev_handle_t device, const uint8_t *tx, size_t length, int timeout);
esp_err_t i2c_master_receive(i2c_master_dev_handle_t device, uint8_t *rx, size_t length, int timeout);
esp_err_t i2c_master_bus_reset(i2c_master_bus_handle_t bus);
