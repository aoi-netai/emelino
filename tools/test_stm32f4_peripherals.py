#!/usr/bin/env python3
"""Link all four drivers and verify pin ownership in one Renode firmware."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from test_stm32f4_gpio import run, value

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/boards/stm32f4-peripherals"


def test(args, report):
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    found = shutil.which(str(Path(args.renode or os.environ.get("RENODE") or "renode").expanduser()))
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    renode = Path(found).resolve()
    generated, firmware = output / "generated", output / "firmware"
    commands = [[args.emelino.resolve(), "build", FIXTURE / "main.emel", "--config", FIXTURE / "config.json",
                 "--out-dir", generated],
                [sys.executable, ROOT / "tools/build_firmware.py",
                 "--generated-dir", generated, "--output-dir", firmware,
                 "--source", FIXTURE / "probe.c", "--optimization", args.optimization]]
    if args.cc:
        commands[1] += ["--cc", args.cc]
    if args.sdk_dir:
        commands[1] += ["--sdk-dir", args.sdk_dir]
    for index, command in enumerate(commands):
        status, log = run(command, cwd=ROOT)
        (output / f"build-{index}.log").write_text(log)
        if status:
            raise ValueError("firmware generation failed:\n" + log)
    manifest = json.loads((firmware / "firmware.json").read_text())
    symbols = json.loads((firmware / "symbols.json").read_text())
    report.update({"board": manifest["board"], "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "resources": manifest["resources"], "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "model": "Renode generic STM32F4, F446RE memory sizes",
                   "model_limitations": ["configuration and pin ownership only; individual driver suites test data transfers",
                                         "SYSCFG is register storage; GPIO OTYPER and UART reserved SR bits are not modeled",
                                         "not STM32F446 silicon or electrical validation"]})
    platform = (renode.parent / "platforms/cpus/stm32f4.repl").read_text()
    platform = "\n".join(line for line in platform.splitlines() if "ApplySVD " not in line) + "\n"
    registration = "flash: Memory.MappedMemory @ sysbus 0x08000000"
    if platform.count(registration) != 1:
        raise ValueError("unsupported Renode platform Flash registration")
    platform = platform.replace(registration, "flash: Memory.MappedMemory @ { sysbus 0x08000000; sysbus 0x00000000 }")
    (output / "stm32f4.repl").write_text(platform)
    expected = {"emela_board_completed": 1, "emela_board_failures": 0,
                "emela_board_boot_initialized": 1, "peripherals_test_checks": 9,
                "peripherals_test_failures": 0, "peripherals_test_last_failure": 0}
    with tempfile.TemporaryDirectory(prefix="emelino-peripherals-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        (output / "machine.repl").write_text(
            f'using "{alias}/stm32f4.repl"\nnvic:\n    systickFrequency: 16000000\n'
            'flash:\n    size: 0x80000\nsram:\n    size: 0x20000\n'
            'syscfg: Memory.MappedMemory @ sysbus 0x40013800\n    size: 0x400\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console', 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  'sysbus SetHookBeforePeripheralWrite sysbus.usart2 "if offset == 4: self.Log(LogLevel.Info, \'EMELINO_TX {0}\', value)"',
                  f'sysbus LoadBinary @{alias}/firmware/firmware.bin 0x08000000',
                  'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset', 'emulation RunFor "0:00:00.100"']
        for name in expected:
            script += [f'echo "FINAL_{name}"', f'sysbus ReadDoubleWord 0x{symbols[name]:x}']
        script += ['echo "FINAL_ODR"', 'sysbus ReadDoubleWord 0x40020014', 'quit']
        (output / "run.resc").write_text("\n".join(script) + "\n")
        status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e",
                           f"include @{alias}/run.resc"], cwd=ROOT,
                          env=dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"), TMPDIR=temporary))
        (output / "renode.log").write_text(log)
    model = (output / "model.log").read_text(encoding="utf-8-sig")
    warnings = [r"spi1: Slave mode is not supported",
                r"gpioPortB: Unhandled write to offset 0x4\. Unhandled bits: \[8\] when writing value 0x100\. Tags: OT8 \(0x1\)\.",
                r"gpioPortB: Unhandled write to offset 0x4\. Unhandled bits: \[9\] when writing value 0x200\. Tags: OT9 \(0x1\)\.",
                r"usart2: Unhandled write to offset 0x0\. Unhandled bits: \[0-2, 8-31\] when writing value 0xFFFFFFBF\. Tags: PE \(0x1\), FE \(0x1\), NF \(0x1\), LBD \(0x1\), CTS \(0x1\), RESERVED \(0x3FFFFF\)\."]
    known = re.compile(r"^.*\[WARNING\] (?:" + "|".join(warnings) + r")\s*$", re.M)
    report["known_model_warnings"] = known.findall(model)
    if status or re.search(r"\[(?:ERROR|WARNING)\]|There was an error executing command|Unhandled Exception", known.sub("", log + model)):
        raise ValueError("unexpected Renode warning/error; see model.log and renode.log")
    metrics = {name: value(log, "FINAL_" + name) for name in expected}
    transmitted = [int(number) for number in re.findall(r"EMELINO_TX (\d+)", model)]
    if metrics != expected or transmitted != [79, 75, 10] or value(log, "FINAL_ODR") & 0x18 != 0x10:
        raise ValueError(f"combined driver checks failed: {metrics}, UART {transmitted}")
    report.update({"status": "passed", "metrics": metrics, "uart_transmitted": transmitted})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--renode")
    parser.add_argument("--cc")
    parser.add_argument("--sdk-dir", type=Path)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out/stm32f4-peripherals")
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    args = parser.parse_args()
    report = {"schema_version": 1, "status": "running", "optimization": args.optimization, "boot": "raw-flash-reset"}
    try:
        test(args, report)
        print("PASS: 9 combined-driver checks, atomic pin claims and UART after rejected reconfiguration")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update({"status": "failed", "error": str(error)})
        print(error, file=sys.stderr)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
