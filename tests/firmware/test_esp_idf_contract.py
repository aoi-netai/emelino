#!/usr/bin/env python3
"""Exercise driver contracts using real generated ABI headers and SDK test doubles.

This verifies ownership, byte packing, error paths and IRQ buffering. It does not
claim hardware or Renode execution. Real vendor compile/link is a separate check.
"""
import argparse
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from firmware.build.profiles import load_profiles
from firmware.ports.esp_idf.build import configure


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="emelino-idf-contract-") as temporary:
        work = Path(temporary)
        generated = work / "generated"
        subprocess.run([str(args.emelino), "build", "examples/esp32c3-peripherals/main.emel",
                        "--config", "examples/esp32c3-peripherals/config.json", "--out-dir", str(generated)], cwd=ROOT, check=True)
        mock = work / "mock"
        profile, chip, _ = load_profiles("esp32-c3-devkitm-1-n4x")
        mock.mkdir()
        configure(mock, profile, chip, 16384, 256)
        for relative in ("freertos/FreeRTOS.h", "freertos/task.h", "driver/gpio.h", "driver/uart.h",
                         "driver/spi_master.h", "driver/i2c_master.h", "esp_intr_alloc.h", "esp_timer.h",
                         "esp_rom_sys.h", "hal/uart_ll.h", "hal/spi_ll.h", "soc/interrupts.h"):
            path = mock / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('#include "esp_idf_mock.h"\n')
        port = ROOT / "firmware/ports/esp_idf"
        tests = Path(__file__).parent
        for optimization in ("O0", "Os"):
            executable = work / optimization
            subprocess.run([args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
                "-fsanitize=undefined", "-fno-sanitize-recover=all", "-" + optimization,
                *["-I" + str(p) for p in (mock, generated, tests, port, ROOT / "firmware/common")],
                *[str(port / (name + ".c")) for name in ("pins", "gpio", "uart", "spi", "i2c", "time")],
                str(ROOT / "firmware/common/spi.c"), str(tests / "esp_idf_contract.c"),
                "-Wl,--wrap=malloc", "-Wl,--wrap=free", "-o", str(executable)], check=True)
            for case in ("gpio", "ownership", "uart", "uart_irq", "spi", "spi_timeout", "spi_config_failure", "i2c", "i2c_failure", "context"):
                subprocess.run([str(executable), case], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
