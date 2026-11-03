//! Check the generated ABI independently of MCU emulation, including bad BSP results.
#[path = "support/heap.rs"]
mod heap;
mod support;

#[test]
fn peripheral_bridges_preserve_nested_words_errors_bounds_and_ownership() {
    let source = r#"
import Embedded.Board.{BoardSpi, BoardUart, BoardI2c}
import Embedded.Spi
import Embedded.Gpio
import Embedded.Uart
import Embedded.I2c

@external(c, "select_mode")
fn select_mode(value: Int) -> () use {}

pub fn transactions(mode: Int) -> Int {
  select_mode(mode)
  with BoardSpi {
    spi = use Spi.Spi
    config = Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits16, Spi.IdleLow,
      Spi.FirstEdge, 1000000, Spi.MostSignificantFirst)
    device = spi.create_device(Spi.SpiDeviceConfig(Spi.Spi1, config,
      Gpio.GpioPin(4), Gpio.Low, Int64.from_int(2), Int64.from_int(3)))
    if mode == 9 { spi.recover(Spi.Spi1)
      0
    } else {
      operations = if mode == 1 { [Spi.Write([-1])] }
        else if mode == 2 { [Spi.Write([1, 2, 3, 4, 5])] }
        else if mode == 3 { [Spi.Read(-1)] }
        else { [Spi.Write([1, 65535]), Spi.Read(2), Spi.Transfer([32768])] }
      result = spi.transaction(device, operations, Int64.from_int(10))
      if result == [[], [9, 10], [11]] { 42 } else { 0 }
    }
  } escape {
    Spi.SpiTransactionError(Some(Spi.SpiFailure(Spi.InvalidArgument)), None) -> 21
    Spi.SpiTransactionError(Some(Spi.SpiFailure(Spi.CapacityExceeded)), None) -> 22
    Spi.SpiTransactionError(Some(Spi.SpiFailure(Spi.Other)), None) -> 24
    Spi.SpiTransactionError(Some(Spi.SpiFailure(Spi.Timeout)), Some(Gpio.InvalidState)) -> 25
    Spi.SpiTransactionError(None, Some(Gpio.Busy)) -> 26
    Spi.SpiTransactionError(Some(Spi.GpioFailure(Gpio.ResourceConflict)), None) -> 27
    Spi.SpiTransactionError(Some(Spi.SpiFailure(Spi.Busy)), Some(Gpio.Other)) -> 29
    Spi.SpiTransactionError(_, _) -> -1
  }
}

pub fn reads(mode: Int) -> Int {
  select_mode(mode)
  with BoardUart, BoardI2c {
    if mode == 10 { List.length((use Uart.Uart).receive(Uart.Uart2, 2, Int64.from_int(10))) }
    else if mode == 11 { List.length((use Uart.Uart).receive_some(Uart.Uart2, 2, Int64.from_int(10))) }
    else {
      List.length((use I2c.I2c).write_read(I2c.I2c1, 80, [],
        if mode == 13 { 0 } else if mode == 14 { 1 } else { 2 }, Int64.from_int(10)))
    }
  } escape {
    Uart.UartError(Uart.Other) -> 41
    I2c.I2cError(I2c.Other) -> 42
    I2c.I2cError(I2c.InvalidArgument) -> 43
    Uart.UartError(_) -> -2
    I2c.I2cError(_) -> -3
  }
}
"#;
    let output = heap::with_heap_audit(support::compile(
        source,
        &[("transactions", "transactions"), ("reads", "reads")],
        &emelino_compiler::Options::default()
            .with_heap_bytes(16384)
            .with_peripheral_buffer_capacity(4),
    ));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
#include "emela_spi.h"
#include "emela_gpio.h"
#include "emela_uart.h"
#include "emela_i2c.h"
static int mode, calls;
void select_mode(int32_t value) { mode = value; calls = 0; }
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  (void)k; (void)m; (void)n; abort();
}
EmelaSpiTransactionStatus emela_spi_create_device(EmelaSpiDeviceConfig c, EmelaSpiDevice *out) {
  assert(c.cs.id == 4 && c.config.max_frequency_hz == 1000000 && c.setup_us == 2 && c.hold_us == 3);
  if (mode == 8) return (EmelaSpiTransactionStatus){.primary_gpio=5};
  out->slot = 7;
  return (EmelaSpiTransactionStatus){0};
}
EmelaSpiTransactionStatus emela_spi_recover(EmelaSpiId id) {
  assert(id == EMELA_SPI_ID_SPI1);
  return (EmelaSpiTransactionStatus){.primary_spi=6, .cleanup_gpio=7};
}
EmelaSpiTransactionStatus emela_spi_transaction(EmelaSpiDevice d, const EmelaSpiOperation *ops,
  uint16_t count, int64_t timeout, uint16_t *out, uint16_t capacity, uint16_t *lengths) {
  calls++;
  assert(d.slot == 7 && count == 3 && timeout == 10 && capacity == 4);
  assert(ops[0].tag == EMELA_SPI_OPERATION_WRITE && ops[0].length == 2);
  assert(ops[0].data[0] == 1 && ops[0].data[1] == 65535);
  assert(ops[1].tag == EMELA_SPI_OPERATION_READ && ops[1].length == 2);
  assert(ops[2].tag == EMELA_SPI_OPERATION_TRANSFER && ops[2].length == 1 && ops[2].data[0] == 32768);
  if (mode == 5) return (EmelaSpiTransactionStatus){.primary_spi=5,.cleanup_gpio=6};
  if (mode == 6) return (EmelaSpiTransactionStatus){.cleanup_gpio=4};
  if (mode == 7) return (EmelaSpiTransactionStatus){.primary_gpio=5};
  lengths[0]=0; lengths[1]=mode==4 ? 1 : 2; lengths[2]=1;
  out[0]=9; out[1]=10; out[2]=11;
  return (EmelaSpiTransactionStatus){0};
}
int32_t emela_uart_receive(EmelaUartId id,int32_t size,int64_t timeout,uint16_t *out,uint16_t cap,uint16_t *length) {
  assert(id==EMELA_UART_ID_UART2 && size==2 && timeout==10 && cap==4);
  out[0]=5; *length=1; return 0;
}
int32_t emela_uart_receive_some(EmelaUartId id,int32_t size,int64_t timeout,uint16_t *out,uint16_t cap,uint16_t *length) {
  return emela_uart_receive(id,size,timeout,out,cap,length);
}
int32_t emela_i2c_write_read(EmelaI2cId id,int32_t address,const uint16_t *data,uint16_t n,
  int32_t size,int64_t timeout,uint16_t *out,uint16_t cap,uint16_t *length) {
  (void)data; calls++;
  assert(id==EMELA_I2C_ID_I2C1 && address==80 && n==0 && size==(mode==14 ? 1 : 2) && timeout==10 && cap==4);
  out[0]=6; *length=1; return 0;
}
/* The effect handler contains all its operations. Unused operations must never run. */
#define UNEXPECTED(signature) signature { abort(); }
UNEXPECTED(int32_t emela_spi_configure(EmelaSpiId id, EmelaSpiConfig c))
UNEXPECTED(int32_t emela_spi_transmit(EmelaSpiId id,const uint16_t*d,uint16_t n,int64_t t))
UNEXPECTED(int32_t emela_spi_receive(EmelaSpiId id,int32_t n,int64_t t,uint16_t*out,uint16_t c,uint16_t*l))
UNEXPECTED(int32_t emela_spi_transfer(EmelaSpiId id,const uint16_t*d,uint16_t n,int64_t t,uint16_t*out,uint16_t c,uint16_t*l))
UNEXPECTED(int32_t emela_uart_configure(EmelaUartId id,EmelaUartConfig c))
UNEXPECTED(int32_t emela_uart_transmit(EmelaUartId id,const uint16_t*d,uint16_t n,int64_t t))
UNEXPECTED(int32_t emela_uart_discard_input(EmelaUartId id,int32_t n,int32_t*out))
UNEXPECTED(int32_t emela_uart_set_receive_mode(EmelaUartId id,EmelaUartReceiveMode m))
UNEXPECTED(int32_t emela_uart_enable_interrupt(EmelaUartId id,EmelaInterruptPriority p,int32_t notify))
UNEXPECTED(int32_t emela_uart_disable_interrupt(EmelaUartId id))
UNEXPECTED(int32_t emela_i2c_configure(EmelaI2cId id,EmelaI2cConfig c))
UNEXPECTED(int32_t emela_i2c_master_transmit(EmelaI2cId id,int32_t a,const uint16_t*d,uint16_t n,int64_t t))
UNEXPECTED(int32_t emela_i2c_memory_write(EmelaI2cId id,int32_t a,int32_t r,EmelaI2cMemoryAddressSize s,const uint16_t*d,uint16_t n,int64_t t))
UNEXPECTED(int32_t emela_i2c_is_device_ready(EmelaI2cId id,int32_t a,int32_t n,int64_t t,int32_t*out))
UNEXPECTED(int32_t emela_i2c_recover(EmelaI2cId id,int64_t t))
UNEXPECTED(int32_t emela_i2c_enable_interrupt(EmelaI2cId id,EmelaInterruptPriority p))
UNEXPECTED(int32_t emela_i2c_disable_interrupt(EmelaI2cId id))
int main(void) {
  const int expected[]={42,21,22,21,24,25,26,27,27,29};
  for(int round=0;round<100;round++) {
    for(int i=0;i<10;i++) {
      assert(transactions(i)==expected[i]);
      assert(calls==((i==0 || (i>=4 && i<=7)) ? 1 : 0));
      assert(test_heap_empty());
    }
    assert(reads(10)==41 && test_heap_empty());
    assert(reads(11)==1 && test_heap_empty());
    assert(reads(12)==42 && calls==1 && test_heap_empty());
    assert(reads(13)==43 && calls==0 && test_heap_empty());
    assert(reads(14)==1 && calls==1 && test_heap_empty());
  }
  return 0;
}
"#,
    );
}
