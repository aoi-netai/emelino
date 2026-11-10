#!/usr/bin/env python3
"""Pico serial error/ownership/deadline contracts; hardware transfers use rp2350js.

This host model deliberately does not implement an I2C peer or CPU/IRQ timing.
It exercises rejection without side effects, SPI frequency bounds, UART buffer
overflow and bounded recovery using the real port and generated public ABI.
"""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
MOCK_HEADERS = {
    "pico/platform.h": """
#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
typedef unsigned int uint;
extern uint mock_core, mock_exception;
static inline uint get_core_num(void) { return mock_core; }
static inline uint __get_current_exception(void) { return mock_exception; }
""",
    "hardware/sync.h": """
#pragma once
#include <stdint.h>
extern uint32_t mock_irq_mask;
void mock_interrupt_resume(void);
static inline uint32_t save_and_disable_interrupts(void) {
    uint32_t previous = mock_irq_mask; mock_irq_mask = 1; return previous;
}
static inline void restore_interrupts(uint32_t previous) {
    mock_irq_mask = previous;
    if (!previous) mock_interrupt_resume();
}
static inline void hw_set_bits(volatile uint32_t *p, uint32_t b) { *p |= b; }
static inline void hw_clear_bits(volatile uint32_t *p, uint32_t b) { *p &= ~b; }
""",
    "hardware/clocks.h": """
#pragma once
#include "pico/platform.h"
enum { clk_sys, clk_peri };
extern uint mock_clock;
static inline uint clock_get_hz(uint clock) { (void)clock; return mock_clock; }
""",
    "hardware/gpio.h": """
#pragma once
#include "pico/platform.h"
enum { GPIO_IN = 0, GPIO_OUT = 1, GPIO_FUNC_SPI = 1, GPIO_FUNC_UART = 2,
       GPIO_FUNC_I2C = 3, GPIO_FUNC_SIO = 5, GPIO_FUNC_NULL = 31 };
void gpio_disable_pulls(uint pin);
void gpio_pull_up(uint pin);
void gpio_set_function(uint pin, uint function);
void gpio_put(uint pin, bool value);
void gpio_set_dir(uint pin, bool output);
bool gpio_get(uint pin);
""",
    "hardware/irq.h": """
#pragma once
#include "pico/platform.h"
enum { UART0_IRQ = 20, I2C0_IRQ = 23 };
void irq_clear(uint irq);
void irq_set_priority(uint irq, uint8_t priority);
void irq_set_enabled(uint irq, bool enabled);
void irq_set_exclusive_handler(uint irq, void (*handler)(void));
""",
    "hardware/uart.h": """
#pragma once
#include "pico/platform.h"
typedef struct { volatile uint32_t dr, rsr, fr, imsc, mis, icr, cr, dmacr; } uart_hw_t;
typedef int uart_inst_t;
typedef int uart_parity_t;
#define uart0 ((uart_inst_t *)0)
extern uart_hw_t mock_uart;
static inline uart_hw_t *uart_get_hw(uart_inst_t *p) { (void)p; return &mock_uart; }
#define UART_CLOCK_NUM(p) clk_peri
#define UART_FUNCSEL_NUM(p, gpio) GPIO_FUNC_UART
#define UART_UARTDR_FE_BITS 0x100u
#define UART_UARTDR_PE_BITS 0x200u
#define UART_UARTDR_BE_BITS 0x400u
#define UART_UARTDR_OE_BITS 0x800u
#define UART_UARTIMSC_RXIM_BITS 0x10u
#define UART_UARTIMSC_TXIM_BITS 0x20u
#define UART_UARTIMSC_RTIM_BITS 0x40u
#define UART_UARTIMSC_FEIM_BITS 0x80u
#define UART_UARTIMSC_PEIM_BITS 0x100u
#define UART_UARTIMSC_BEIM_BITS 0x200u
#define UART_UARTIMSC_OEIM_BITS 0x400u
#define UART_UARTMIS_TXMIS_BITS 0x20u
#define UART_UARTICR_TXIC_BITS 0x20u
#define UART_UARTICR_BITS 0x7ffu
#define UART_UARTCR_UARTEN_BITS 1u
#define UART_UARTCR_TXE_BITS 0x100u
#define UART_UARTCR_RXE_BITS 0x200u
#define UART_UARTFR_BUSY_BITS 8u
enum { UART_PARITY_NONE, UART_PARITY_EVEN, UART_PARITY_ODD };
uint uart_init(uart_inst_t *p, uint baud);
void uart_set_fifo_enabled(uart_inst_t *p, bool enabled);
void uart_set_format(uart_inst_t *p, uint data, uint stop, uart_parity_t parity);
bool uart_is_readable(uart_inst_t *p);
bool uart_is_writable(uart_inst_t *p);
""",
    "hardware/spi.h": """
#pragma once
#include "pico/platform.h"
typedef struct { volatile uint32_t dr, cr0, cr1, cpsr, imsc, dmacr, icr, ris, sr; } spi_hw_t;
typedef int spi_inst_t;
#define spi0 ((spi_inst_t *)0)
extern spi_hw_t mock_spi;
static inline spi_hw_t *spi_get_hw(spi_inst_t *p) { (void)p; return &mock_spi; }
#define SPI_SSPCR0_SCR_LSB 8u
#define SPI_SSPCR0_SPO_BITS 0x40u
#define SPI_SSPCR0_SPH_BITS 0x80u
#define SPI_SSPCR1_SSE_BITS 2u
#define SPI_SSPICR_RORIC_BITS 1u
#define SPI_SSPICR_RTIC_BITS 2u
#define SPI_SSPRIS_RORRIS_BITS 1u
#define SPI_SSPSR_BSY_BITS 0x10u
uint spi_init(spi_inst_t *p, uint baud);
bool spi_is_writable(spi_inst_t *p);
bool spi_is_readable(spi_inst_t *p);
""",
    "hardware/i2c.h": """
#pragma once
#include "pico/platform.h"
typedef struct { volatile uint32_t intr_mask, dma_cr, clr_intr, raw_intr_stat,
 tx_abrt_source, rxflr, data_cmd, clr_stop_det, status, enable, enable_status, tar; } i2c_hw_t;
typedef int i2c_inst_t;
#define i2c0 ((i2c_inst_t *)0)
extern i2c_hw_t mock_i2c;
static inline i2c_hw_t *i2c_get_hw(i2c_inst_t *p) { (void)p; return &mock_i2c; }
#define I2C_IC_INTR_MASK_M_TX_ABRT_BITS 0x40u
#define I2C_IC_INTR_MASK_M_TX_OVER_BITS 8u
#define I2C_IC_INTR_MASK_M_RX_OVER_BITS 2u
#define I2C_IC_INTR_MASK_M_RX_UNDER_BITS 1u
#define I2C_IC_INTR_MASK_M_STOP_DET_BITS 0x200u
#define I2C_IC_INTR_MASK_M_RX_FULL_BITS 4u
#define I2C_IC_INTR_MASK_M_TX_EMPTY_BITS 0x10u
#define I2C_IC_RAW_INTR_STAT_TX_ABRT_BITS 0x40u
#define I2C_IC_RAW_INTR_STAT_TX_EMPTY_BITS 0x10u
#define I2C_IC_RAW_INTR_STAT_STOP_DET_BITS 0x200u
#define I2C_IC_TX_ABRT_SOURCE_ARB_LOST_BITS 0x1000u
#define I2C_IC_TX_ABRT_SOURCE_ABRT_7B_ADDR_NOACK_BITS 1u
#define I2C_IC_TX_ABRT_SOURCE_ABRT_TXDATA_NOACK_BITS 8u
#define I2C_IC_TX_ABRT_SOURCE_ABRT_GCALL_NOACK_BITS 0x10u
#define I2C_IC_STATUS_TFNF_BITS 2u
#define I2C_IC_DATA_CMD_STOP_BITS 0x200u
#define I2C_IC_DATA_CMD_CMD_BITS 0x100u
#define I2C_IC_DATA_CMD_RESTART_BITS 0x400u
#define I2C_IC_ENABLE_STATUS_IC_EN_BITS 1u
uint i2c_init(i2c_inst_t *p, uint baud);
void i2c_deinit(i2c_inst_t *p);
""",
    "board_config.h": """
#pragma once
#define EMELA_GPIO_PIN_MASK UINT32_C(0x1e7fffff)
#define EMELA_UART0_TX_PIN 0
#define EMELA_UART0_RX_PIN 1
#define EMELA_SPI1_SCK_PIN 18
#define EMELA_SPI1_MISO_PIN 16
#define EMELA_SPI1_MOSI_PIN 19
#define EMELA_I2C1_SCL_PIN 5
#define EMELA_I2C1_SDA_PIN 4
""",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="emelino-pico-serial-") as directory:
        work = Path(directory)
        generated = work / "generated"
        fixture = ROOT / "tests/embedded/scenarios/pico_peripherals"
        subprocess.run([str(args.emelino), "build", str(fixture / "main.emel"),
            "--config", str(fixture / "config.json"), "--out-dir", str(generated)], cwd=ROOT, check=True)
        mock = work / "mock"
        for relative, contents in MOCK_HEADERS.items():
            path = mock / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents)
        port = ROOT / "firmware/ports/pico_sdk"
        for optimization in ("O0", "Os"):
            binary = work / ("test-" + optimization)
            subprocess.run([args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror",
                "-fsanitize=undefined", "-fno-sanitize-recover=all", "-" + optimization,
                "-I" + str(mock), "-I" + str(generated), "-I" + str(port),
                "-I" + str(ROOT / "firmware/common"), str(port / "pins.c"),
                str(port / "uart.c"), str(port / "spi.c"), str(port / "i2c.c"),
                str(ROOT / "firmware/common/spi.c"),
                str(Path(__file__).with_name("pico_serial.c")), "-o", str(binary)], check=True)
            for case in ("uart", "uart_irq", "spi", "i2c", "context"):
                subprocess.run([str(binary), case], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
