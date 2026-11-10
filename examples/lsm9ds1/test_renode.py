#!/usr/bin/env python3
"""Run the built STM32 demo against Renode's unmodified LSM9DS1 model."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT), str(ROOT / "tests/embedded")]
from support.common import MODELS, load_module, load_target, renode_workspace, run_renode, value


def test(args, report):
    found = shutil.which(args.renode)
    if not found:
        raise ValueError("Renode not found; use --renode or set RENODE")
    renode = Path(found).resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "uart.log"):
        (output / name).unlink(missing_ok=True)
    firmware = args.firmware_dir.resolve()
    symbols = json.loads((firmware / "symbols.json").read_text())
    manifest = json.loads((firmware / "firmware.json").read_text())
    if manifest["board"] != "nucleo-f446re":
        raise ValueError("this demo uses the nucleo-f446re wiring")
    report.update(firmware_sha256=manifest["sha256"]["firmware.bin"],
                  sensor_model="Renode built-in Sensors.LSM9DS1_IMU",
                  model_limitations=["generic STM32F4, not STM32F446 silicon timing",
                                     "sample queues advance on reads, not on ODR time",
                                     "BDU, power modes and physical-unit sensitivity are simplified"])
    (output / "firmware").mkdir(exist_ok=True)
    shutil.copyfile(firmware / "firmware.bin", output / "firmware/firmware.bin")
    target = load_target("nucleo-f446re")
    platform = load_module(ROOT / "tests/embedded/ports/stm32f4/renode.py", "lsm9ds1_platform").Platform(target)
    platform.prepare(renode, output, report, MODELS / "EmelinoSTM32F4I2C.cs", 1)
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(platform.machine_header(alias) +
            "imu: Sensors.LSM9DS1_IMU @ i2c1 0x6b\n"
            "syscfg: Memory.MappedMemory @ sysbus 0x40013800\n    size: 0x400\n")
        script = [f"logFile @{alias}/model.log", "logLevel 1 file", "logLevel 3 console",
                  *platform.controller_includes(alias), "mach create",
                  f"machine LoadPlatformDescription @{alias}/machine.repl",
                  "sysbus.gpioPortB OnGPIO 8 true", "sysbus.gpioPortB OnGPIO 9 true",
                  platform.load_binary(alias), "sysbus.cpu PerformanceInMips 100",
                  f"sysbus.usart2 CreateFileBackend @{alias}/uart.log true", "sysbus.cpu Reset"]
        for i in range(5):
            script += [f"sysbus.i2c1.imu FeedAccelerationSample {i / 4} -1 0.5",
                       f"sysbus.i2c1.imu FeedAngularRateSample {i * 10} -10 0.5"]
        script.append('emulation RunFor "0:00:01"')
        for name in ("emela_board_completed", "emela_board_failures", "emela_board_boot_initialized"):
            script += [f'echo "{name}"', f'sysbus ReadDoubleWord {symbols[name]:#x}']
        script.append("quit")
        status, log = run_renode(renode, output, alias, temporary, script, report, 60)
    model = (output / "model.log").read_text(encoding="utf-8-sig")
    if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception|Errors during compilation", log + model):
        raise ValueError("Renode failed; see renode.log/model.log")
    warnings = [line.split("[WARNING] ", 1)[1] for line in model.splitlines() if "[WARNING] " in line]
    known = {
        f"gpioPortB: Unhandled write to offset 0x4. Unhandled bits: [{pin}] when writing value 0x{1 << pin:X}. Tags: OT{pin} (0x1)."
        for pin in (8, 9)
    } | {
        "i2c1.imu: Unhandled write to offset 0x22. Unhandled bits: [6] when writing value 0x44. Tags: BDU: Block data update (0x1).",
        "i2c1.imu: Unhandled write to offset 0x20. Unhandled bits: [5-6] when writing value 0x60. Tags: ODR_XL: Accelerometer output data rate (0x3).",
        "i2c1.imu: Unhandled write to offset 0x10. Unhandled bits: [5-6] when writing value 0x60. Tags: ODR: Gyroscope output data rate (0x3).",
        "usart2: Unhandled write to offset 0x0. Unhandled bits: [0-2, 8-31] when writing value 0xFFFFFFBF. Tags: PE (0x1), FE (0x1), NF (0x1), LBD (0x1), CTS (0x1), RESERVED (0x3FFFFF).",
    }
    unknown = [warning for warning in warnings if warning not in known]
    if unknown:
        raise ValueError("unexpected model warning: " + "\n".join(unknown))
    metrics = {name: value(log, name) for name in (
        "emela_board_completed", "emela_board_failures", "emela_board_boot_initialized")}
    if list(metrics.values()) != [1, 0, 1]:
        raise ValueError(f"firmware failed: {metrics}")
    expected = "LSM9DS1 raw 6-axis\r\n" + "".join(
        f"accel {i * 4096} -16384 8192\r\ngyro {i * 1200} -1200 60\r\n" for i in range(5)) + "power down\r\n"
    actual = (output / "uart.log").read_bytes()
    if actual != expected.encode("ascii"):
        raise ValueError(f"unexpected UART output: {actual!r}")
    report.update(status="passed", metrics=metrics, samples=5,
                  uart_output=actual.decode("ascii"),
                  known_model_warnings=warnings)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--renode", default=os.environ.get("RENODE", "renode"))
    parser.add_argument("--firmware-dir", type=Path, default=ROOT / "out/lsm9ds1/firmware")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out/lsm9ds1/renode")
    args = parser.parse_args()
    report = {"status": "failed"}
    try:
        test(args, report)
    except (ValueError, OSError, KeyError) as error:
        report["error"] = str(error)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if report["status"] != "passed":
        sys.exit(report.get("error", "test failed"))
    print("LSM9DS1: 5 samples passed (STM32F446 firmware, Renode model)")
