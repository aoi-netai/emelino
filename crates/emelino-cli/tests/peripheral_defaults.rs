//! Shorthand must reach the existing backend as the corresponding explicit call.
mod support;

use emelino_cli::{
    config::{Config, Export},
    frontend,
};
use emelino_compiler::{Options, Output};
use std::path::Path;

fn compile(source: &str) -> Output {
    support::compile(
        source,
        &[("entry", "entry")],
        &Options::default().with_heap_bytes(16384),
    )
}

fn same_output(short: &Output, explicit: &Output) {
    for (left, right) in short.files.iter().zip(&explicit.files) {
        assert_eq!(left.path, right.path);
        if left.contents != right.contents {
            let differing = left
                .contents
                .lines()
                .zip(right.contents.lines())
                .enumerate()
                .find(|(_, (left, right))| left != right);
            panic!("different generated artifact {}: {differing:?}", left.path);
        }
    }
    assert!(short == explicit, "shorthand changed generated artifacts");
}

const CONFIGS: &str = r#"
import Embedded.Gpio
import Embedded.Uart
import Embedded.I2c
import Embedded.Spi

pub fn entry() -> Int {
  input = Gpio.Input()
  assert input match { Gpio.Input(Gpio.NoPull) -> True
    _ -> False }
  push_pull = Gpio.OutputPushPull()
  assert push_pull match { Gpio.OutputPushPull(Gpio.NoPull, Gpio.Low) -> True
    _ -> False }
  open_drain = Gpio.OutputOpenDrain()
  assert open_drain match { Gpio.OutputOpenDrain(Gpio.NoPull, Gpio.High) -> True
    _ -> False }
  analog = Gpio.Analog()
  assert analog match { Gpio.Analog(Gpio.NoPull) -> True
    _ -> False }
  uart = Uart.UartConfig(115200)
  assert uart match {
    Uart.UartConfig(115200, Uart.Bits8, Uart.One, Uart.NoParity, Uart.Both,
      Uart.NoFlowControl, Uart.Native) -> True
    _ -> False
  }
  i2c = I2c.I2cConfig()
  assert i2c match { I2c.I2cConfig(100000, I2c.Standard) -> True
    _ -> False }
  spi = Spi.SpiConfig()
  assert spi match {
    Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8, Spi.IdleLow, Spi.FirstEdge,
      4000000, Spi.MostSignificantFirst) -> True
    _ -> False
  }
  device = Spi.SpiDeviceConfig(Spi.Spi1, cs: Gpio.GpioPin(4))
  assert device match {
    Spi.SpiDeviceConfig(Spi.Spi1,
      Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8, Spi.IdleLow, Spi.FirstEdge,
        4000000, Spi.MostSignificantFirst), Gpio.GpioPin(4), Gpio.Low, 0, 0) -> True
    _ -> False
  }
  # Overrides and record updates retain their ordinary meanings.
  cfg = Uart.UartConfig(9600, parity: Uart.Even, direction: Uart.Receive)
  assert cfg match {
    Uart.UartConfig(9600, Uart.Bits8, Uart.One, Uart.Even, Uart.Receive,
      Uart.NoFlowControl, Uart.Native) -> True
    _ -> False
  }
  changed = Uart.UartConfig(baud_rate: 4800, ..cfg)
  assert changed.parity match { Uart.Even -> True
    _ -> False }
  assert changed.baud_rate == 4800
  assert Gpio.OutputPushPull(initial: Gpio.High, pull: Gpio.Up) match {
    Gpio.OutputPushPull(Gpio.Up, Gpio.High) -> True
    _ -> False
  }
  slow = Spi.SpiConfig(max_frequency_hz: 1000000)
  assert slow.max_frequency_hz == 1000000
  42
}
"#;

const CONFIG_REPLACEMENTS: &[(&str, &str)] = &[
    ("Gpio.Input()", "Gpio.Input(Gpio.NoPull)"),
    (
        "Gpio.OutputPushPull()",
        "Gpio.OutputPushPull(Gpio.NoPull, Gpio.Low)",
    ),
    (
        "Gpio.OutputOpenDrain()",
        "Gpio.OutputOpenDrain(Gpio.NoPull, Gpio.High)",
    ),
    ("Gpio.Analog()", "Gpio.Analog(Gpio.NoPull)"),
    (
        "Uart.UartConfig(115200)",
        "Uart.UartConfig(115200, Uart.Bits8, Uart.One, Uart.NoParity, Uart.Both, Uart.NoFlowControl, Uart.Native)",
    ),
    ("I2c.I2cConfig()", "I2c.I2cConfig(100000, I2c.Standard)"),
    (
        "Spi.SpiConfig()",
        "Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8, Spi.IdleLow, Spi.FirstEdge, 4000000, Spi.MostSignificantFirst)",
    ),
    (
        "Spi.SpiDeviceConfig(Spi.Spi1, cs: Gpio.GpioPin(4))",
        "Spi.SpiDeviceConfig(Spi.Spi1, cs: Gpio.GpioPin(4), config: Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8, Spi.IdleLow, Spi.FirstEdge, 4000000, Spi.MostSignificantFirst), active: Gpio.Low, setup_us: 0, hold_us: 0)",
    ),
    (
        "Uart.UartConfig(9600, parity: Uart.Even, direction: Uart.Receive)",
        "Uart.UartConfig(9600, parity: Uart.Even, direction: Uart.Receive, data_bits: Uart.Bits8, stop_bits: Uart.One, flow_control: Uart.NoFlowControl, oversampling: Uart.Native)",
    ),
    (
        "Spi.SpiConfig(max_frequency_hz: 1000000)",
        "Spi.SpiConfig(max_frequency_hz: 1000000, mode: Spi.Master, direction: Spi.TwoLine, data_size: Spi.Bits8, polarity: Spi.IdleLow, phase: Spi.FirstEdge, bit_order: Spi.MostSignificantFirst)",
    ),
];

fn expanded(source: &str, replacements: &[(&str, &str)]) -> String {
    let mut result = source.to_owned();
    for (short, full) in replacements {
        assert!(result.contains(short), "missing shorthand fixture: {short}");
        result = result.replace(short, full);
    }
    result
}

#[test]
fn constructors_preserve_values_overrides_and_record_updates() {
    let output = compile(CONFIGS);
    same_output(&output, &compile(&expanded(CONFIGS, CONFIG_REPLACEMENTS)));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
int main(void) { assert(entry() == 42); return 0; }
"#,
    );
}

const OPERATIONS: &str = r#"
import Embedded.Board.{BoardGpio, BoardUart, BoardI2c, BoardSpi}
import Embedded.Gpio
import Embedded.Uart
import Embedded.I2c
import Embedded.Spi
import Embedded.Interrupt

fn callback(event: Gpio.GpioEvent) -> () { () }

pub fn entry() -> Int {
  with BoardGpio, BoardUart, BoardI2c, BoardSpi {
    gpio = use Gpio.Gpio
    uart = use Uart.Uart
    i2c = use I2c.I2c
    spi = use Spi.Spi
    gpio.configure(Gpio.GpioPin(0), Gpio.Input())
    gpio.enable_interrupt(Gpio.GpioPin(0), Gpio.Rising, on_event: callback)
    uart.configure(Uart.Uart2, Uart.UartConfig(115200))
    uart.transmit(Uart.Uart2, [42])
    received = uart.receive(Uart.Uart2, 1)
    assert received == [42]
    received_some = uart.receive_some(Uart.Uart2, 2)
    assert received_some == [42]
    uart.enable_interrupt(Uart.Uart2)
    i2c.configure(I2c.I2c1)
    i2c.master_transmit(I2c.I2c1, 80, [42])
    read = i2c.write_read(I2c.I2c1, 80, read_size: 1)
    assert read == [42]
    i2c.memory_write(I2c.I2c1, 80, 3, data: [42])
    ready = i2c.is_device_ready(I2c.I2c1, 80)
    assert ready
    i2c.recover(I2c.I2c1)
    i2c.enable_interrupt(I2c.I2c1)
    spi.configure(Spi.Spi1)
    spi.transmit(Spi.Spi1, [42])
    spi_received = spi.receive(Spi.Spi1, 1)
    assert spi_received == [42]
    transferred = spi.transfer(Spi.Spi1, [42])
    assert transferred == [42]
    device = spi.create_device(Spi.SpiDeviceConfig(Spi.Spi1, cs: Gpio.GpioPin(4)))
    transaction = spi.transaction(device, [Spi.Read(1)])
    assert transaction == [[42]]
    gpio.disable_interrupt(Gpio.GpioPin(0))
    42
  } escape {
    Gpio.GpioError(_) -> -1
    Uart.UartError(_) -> -2
    I2c.I2cError(_) -> -3
    Spi.SpiError(_) -> -4
    Spi.SpiTransactionError(_, _) -> -5
  }
}
"#;

const OP_REPLACEMENTS: &[(&str, &str)] = &[
    (
        "on_event: callback)",
        "on_event: callback, priority: Interrupt.Normal)",
    ),
    (
        "uart.transmit(Uart.Uart2, [42])",
        "uart.transmit(Uart.Uart2, [42], 1000)",
    ),
    (
        "uart.receive(Uart.Uart2, 1)",
        "uart.receive(Uart.Uart2, 1, 1000)",
    ),
    (
        "uart.receive_some(Uart.Uart2, 2)",
        "uart.receive_some(Uart.Uart2, 2, 1000)",
    ),
    (
        "uart.enable_interrupt(Uart.Uart2)",
        "uart.enable_interrupt(Uart.Uart2, Interrupt.Normal, False)",
    ),
    (
        "i2c.configure(I2c.I2c1)",
        "i2c.configure(I2c.I2c1, I2c.I2cConfig(100000, I2c.Standard))",
    ),
    (
        "i2c.master_transmit(I2c.I2c1, 80, [42])",
        "i2c.master_transmit(I2c.I2c1, 80, [42], 1000)",
    ),
    (
        "i2c.write_read(I2c.I2c1, 80, read_size: 1)",
        "i2c.write_read(I2c.I2c1, 80, read_size: 1, write_data: [], timeout_ms: 1000)",
    ),
    (
        "i2c.memory_write(I2c.I2c1, 80, 3, data: [42])",
        "i2c.memory_write(I2c.I2c1, 80, 3, data: [42], address_size: I2c.Bits8, timeout_ms: 1000)",
    ),
    (
        "i2c.is_device_ready(I2c.I2c1, 80)",
        "i2c.is_device_ready(I2c.I2c1, 80, 1, 1000)",
    ),
    ("i2c.recover(I2c.I2c1)", "i2c.recover(I2c.I2c1, 1000)"),
    (
        "i2c.enable_interrupt(I2c.I2c1)",
        "i2c.enable_interrupt(I2c.I2c1, Interrupt.Normal)",
    ),
    (
        "spi.configure(Spi.Spi1)",
        "spi.configure(Spi.Spi1, Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8, Spi.IdleLow, Spi.FirstEdge, 4000000, Spi.MostSignificantFirst))",
    ),
    (
        "spi.transmit(Spi.Spi1, [42])",
        "spi.transmit(Spi.Spi1, [42], 1000)",
    ),
    ("spi.receive(Spi.Spi1, 1)", "spi.receive(Spi.Spi1, 1, 1000)"),
    (
        "spi.transfer(Spi.Spi1, [42])",
        "spi.transfer(Spi.Spi1, [42], 1000)",
    ),
    (
        "spi.transaction(device, [Spi.Read(1)])",
        "spi.transaction(device, [Spi.Read(1)], 1000)",
    ),
];

#[test]
fn every_defaulted_operation_matches_explicit_arguments_and_c_abi() {
    let output = compile(OPERATIONS);
    let explicit = expanded(
        &expanded(
            OPERATIONS,
            &[
                CONFIG_REPLACEMENTS[0],
                CONFIG_REPLACEMENTS[4],
                CONFIG_REPLACEMENTS[7],
            ],
        ),
        OP_REPLACEMENTS,
    );
    same_output(&output, &compile(&explicit));
    support::compile_and_run(&output, include_str!("fixtures/peripheral_defaults.c"));
}

#[test]
fn supplied_arguments_are_evaluated_once_in_written_order() {
    let source = OPERATIONS
        .replace("fn callback", "@external(c, \"record_argument\")\nfn record_argument(index: Int, value: Int) -> Int use {}\n\nfn callback")
        .replace("Uart.UartConfig(115200)", "Uart.UartConfig(baud_rate: record_argument(1, 115200), direction: if record_argument(2, 1) == 1 { Uart.Both } else { Uart.Receive })")
        .replace("uart.transmit(Uart.Uart2, [42])", "uart.transmit(data: [record_argument(3, 42)], id: if record_argument(4, 2) == 2 { Uart.Uart2 } else { Uart.Uart0 })")
        .replace("read_size: 1)", "read_size: record_argument(5, 1))");
    let output = compile(&source);
    support::compile_and_run(
        &output,
        &format!(
            "#define EXPECT_ARGUMENTS 5\n{}\nint32_t record_argument(int32_t index, int32_t value) {{ assert((unsigned)index == ++arguments); return value; }}\n",
            include_str!("fixtures/peripheral_defaults.c")
        ),
    );
}

#[test]
fn explicit_operation_options_are_not_replaced() {
    let source = OPERATIONS
        .replace(
            "uart.transmit(Uart.Uart2, [42])",
            "uart.transmit(Uart.Uart2, [42], 7)",
        )
        .replace(
            "uart.enable_interrupt(Uart.Uart2)",
            "uart.enable_interrupt(Uart.Uart2, notify_transmit: True, priority: Interrupt.High)",
        )
        .replace(
            "i2c.is_device_ready(I2c.I2c1, 80)",
            "i2c.is_device_ready(I2c.I2c1, 80, trials: 3)",
        )
        .replace(
            "spi.transaction(device, [Spi.Read(1)])",
            "spi.transaction(device, [Spi.Read(1)], timeout_ms: 9)",
        );
    support::compile_and_run(
        &compile(&source),
        &format!(
            "#define UART_TRANSMIT_TIMEOUT 7\n#define UART_PRIORITY EMELA_INTERRUPT_PRIORITY_HIGH\n#define UART_NOTIFY 1\n#define I2C_READY_TRIALS 3\n#define SPI_TRANSACTION_TIMEOUT 9\n{}",
            include_str!("fixtures/peripheral_defaults.c")
        ),
    );
}

#[test]
fn missing_required_arguments_and_invalid_calls_still_fail() {
    for (body, diagnostic) in [
        ("Uart.UartConfig()", "WrongArgCount"),
        ("Spi.SpiDeviceConfig(Spi.Spi1)", "WrongArgCount"),
        ("Uart.UartConfig(115200, baud_rate: 9600)", "DuplicateArg"),
        (
            "(use I2c.I2c).write_read(I2c.I2c1, 80, read_size: 1, read_size: 2)",
            "DuplicateArg",
        ),
        ("Spi.SpiConfig(frequency: 1)", "UnknownLabel"),
        ("Spi.SpiConfig(max_frequency_hz: False)", "Mismatch"),
        ("Gpio.Input(Gpio.NoPull, Gpio.Up)", "WrongArgCount"),
        ("(use Uart.Uart).configure(Uart.Uart2)", "WrongArgCount"),
        ("(use I2c.I2c).write_read(I2c.I2c1, 80)", "WrongArgCount"),
        (
            "(use Gpio.Gpio).enable_interrupt(Gpio.GpioPin(0), Gpio.Rising)",
            "WrongArgCount",
        ),
        ("(use Clock.Clock).sleep()", "WrongArgCount"),
        ("(use Console.Console).print_line()", "WrongArgCount"),
    ] {
        let source = format!(
            "import Embedded.Uart\nimport Embedded.Spi\nimport Embedded.Gpio\nimport Embedded.I2c\nimport Std.Clock\nimport Std.Console\nfn example() {{ {body} }}\npub fn entry() -> Int {{42}}"
        );
        let error = frontend::generate(&source, Path::new("main.emel"))
            .err()
            .unwrap_or_else(|| panic!("accepted invalid call: {body}"));
        assert!(error.contains(diagnostic), "{body}: {error}");
    }
}

#[test]
fn unrelated_names_and_function_values_keep_their_full_arity() {
    for source in [
        "type UartConfig(baud_rate: Int, data_bits: Int)\npub fn entry() -> Int { UartConfig(115200).baud_rate }",
        "import Embedded.Uart\npub fn entry() -> Int { make = Uart.UartConfig\n make(115200).baud_rate }",
        "effect Other { fn configure(id: Int, config: Int) -> () }\nfn setup() use Other { (use Other).configure(1) }\npub fn entry() -> Int {42}",
    ] {
        let error = frontend::generate(source, Path::new("main.emel"))
            .err()
            .expect("unrelated call unexpectedly acquired defaults");
        assert!(error.contains("WrongArgCount"), "{error}");
    }
}

#[test]
fn defaults_follow_resolved_imports_capabilities_and_pipes() {
    let source = r#"
import Embedded.Uart.{UartConfig}
import Embedded.Uart
import Embedded.I2c
import Embedded.Board.{BoardI2c}
pub fn entry() -> Int {
  config = 115200 |> UartConfig
  assert config.baud_rate == 115200
  with BoardI2c {
    first = use I2c.I2c
    second = first
    I2c.I2c1 |> second.configure
    I2c.I2c1 |> second.write_read(80, read_size: 1)
    42
  } escape { I2c.I2cError(_) -> -1 }
}
"#;
    let explicit = source
        .replace("115200 |> UartConfig", "115200 |> UartConfig(data_bits: Uart.Bits8, stop_bits: Uart.One, parity: Uart.NoParity, direction: Uart.Both, flow_control: Uart.NoFlowControl, oversampling: Uart.Native)")
        .replace("second.configure\n", "second.configure(config: I2c.I2cConfig(100000, I2c.Standard))\n")
        .replace("second.write_read(80, read_size: 1)", "second.write_read(80, [], 1, 1000)");
    same_output(&compile(source), &compile(&explicit));
}

#[test]
fn omitted_priority_keeps_gpio_callback_isr_validation() {
    let source = r#"
import Embedded.Board.{BoardGpio}
import Embedded.Gpio
@external(c, "untrusted")
fn untrusted() -> () use {}
fn callback(event: Gpio.GpioEvent) -> () { untrusted() }
pub fn entry() -> Int {
  with BoardGpio {
    (use Gpio.Gpio).enable_interrupt(Gpio.GpioPin(0), Gpio.Rising, on_event: callback)
    42
  } escape { Gpio.GpioError(_) -> -1 }
}
"#;
    let error = emelino_cli::compile(
        source,
        &Config {
            exports: vec![Export {
                function: "entry".into(),
                symbol: "entry".into(),
            }],
            heap_bytes: Some(4096),
            ..Config::default()
        },
    )
    .unwrap_err();
    assert!(
        error.contains("untrusted") && error.contains("ISR-safe"),
        "{error}"
    );
}
