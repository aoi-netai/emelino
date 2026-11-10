#!/usr/bin/env python3
"""Check Pico GPIO ownership/IRQ/open-drain contracts with generated ABI headers.

SDK register operations are replaced by a small host model. Real SDK compile/link
and rp2350js tests remain separate checks; this does not emulate either MCU.
"""
import argparse
import json
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
static inline uint32_t save_and_disable_interrupts(void) {
    uint32_t previous = mock_irq_mask; mock_irq_mask = 1; return previous;
}
static inline void restore_interrupts(uint32_t previous) { mock_irq_mask = previous; }
static inline void __dsb(void) {}
static inline void __isb(void) {}
""",
    "hardware/gpio.h": """
#pragma once
#include "pico/platform.h"
enum { GPIO_IN = 0, GPIO_FUNC_SIO = 5, GPIO_FUNC_NULL = 31,
       GPIO_IRQ_EDGE_FALL = 4, GPIO_IRQ_EDGE_RISE = 8 };
typedef void (*gpio_irq_callback_t)(uint pin, uint32_t events);
void gpio_init(uint pin);
void gpio_set_pulls(uint pin, bool up, bool down);
void gpio_disable_pulls(uint pin);
void gpio_set_function(uint pin, uint function);
void gpio_set_input_enabled(uint pin, bool enabled);
void gpio_put(uint pin, bool value);
void gpio_set_dir(uint pin, bool output);
bool gpio_get(uint pin);
void gpio_set_irq_enabled(uint pin, uint32_t events, bool enabled);
void gpio_set_irq_callback(gpio_irq_callback_t callback);
""",
    "hardware/irq.h": """
#pragma once
#include "pico/platform.h"
enum { IO_IRQ_BANK0 = 13 };
void irq_clear(uint irq);
void irq_set_priority(uint irq, uint8_t priority);
void irq_set_enabled(uint irq, bool enabled);
""",
    "board_config.h": """
#pragma once
#define EMELA_GPIO_PIN_MASK UINT32_C(0x1e7fffff)
#define EMELA_ANALOG_PIN_MASK UINT32_C(0x1c000000)
""",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="emelino-pico-gpio-") as directory:
        work = Path(directory)
        source = work / "main.emel"
        source.write_text('''import Embedded.Board.{BoardGpio}
import Embedded.Gpio
pub fn main() -> () {
  with BoardGpio {
    (use Gpio.Gpio).configure(Gpio.GpioPin(25), Gpio.OutputPushPull(Gpio.NoPull, Gpio.Low))
  } escape { Gpio.GpioError(_) -> () }
}
''')
        config = work / "config.json"
        config.write_text(json.dumps({"entry": "main", "heap_bytes": 4096,
            "exports": [{"function": "main", "symbol": "test_main"}]}))
        generated = work / "generated"
        subprocess.run([str(args.emelino), "build", str(source), "--config", str(config),
                        "--out-dir", str(generated)], cwd=ROOT, check=True)
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
                str(port / "pins.c"), str(port / "gpio.c"),
                str(Path(__file__).with_name("pico_gpio.c")), "-o", str(binary)], check=True)
            for case in ("pins", "output", "analog", "interrupt", "context"):
                subprocess.run([str(binary), case], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
