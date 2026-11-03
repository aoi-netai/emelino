#!/usr/bin/env python3
"""Build the Emela SPI fixture and exercise its HAL driver in Renode."""

import argparse
import hashlib
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
FIXTURE = ROOT / "tests/boards/stm32f4-spi"


def test(args, report):
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    requested = str(Path(args.renode or os.environ.get("RENODE") or "renode").expanduser())
    found = shutil.which(requested)
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    renode = Path(found).resolve()
    generated, firmware = output / "generated", output / "firmware"
    commands = [[args.emelino.resolve(), "build", FIXTURE / "main.emel", "--config", FIXTURE / "config.json",
                 "--out-dir", generated],
                [sys.executable, ROOT / "boards/stm32/stm32f4/build.py",
                 "--generated-dir", generated, "--output-dir", firmware,
                 "--source", FIXTURE / "probe.c", "--optimization", args.optimization,
                 "--link-arg=-Wl,--wrap=HAL_SPI_TransmitReceive",
                 "--link-arg=-Wl,--wrap=stm32f4_gpio_write_owned"]]
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
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"]})
    source_platform = renode.parent / "platforms/cpus/stm32f4.repl"
    platform = source_platform.read_text()
    report["platform_sha256"] = hashlib.sha256(source_platform.read_bytes()).hexdigest()
    report["model"] = "Renode generic STM32F4, SPI1 deterministic byte peer, F446RE memory sizes"
    report["model_limitations"] = ["not an STM32F446 silicon model",
                                   "SPI peer exchanges bytes only; 16-bit DR writes are observed separately",
                                   "SCK frequency, CPOL/CPHA and bit order are register configuration checks",
                                   "CS setup/hold uses virtual time; electrical timing is not validated",
                                   "primary/cleanup combination uses test-only linker wrappers",
                                   "HAL polling timeout is tested by withholding RXNE with a bus read hook"]
    platform = "\n".join(line for line in platform.splitlines() if "ApplySVD " not in line) + "\n"
    registration = "flash: Memory.MappedMemory @ sysbus 0x08000000"
    if platform.count(registration) != 1:
        raise ValueError("unsupported Renode platform Flash registration")
    platform = platform.replace(registration, "flash: Memory.MappedMemory @ { sysbus 0x08000000; sysbus 0x00000000 }")
    (output / "stm32f4.repl").write_text(platform)
    shutil.copyfile(FIXTURE / "SpiPeer.cs", output / "SpiPeer.cs")
    names = ["emela_board_completed", "emela_board_failures", "emela_board_boot_initialized",
             "spi_test_stage", "spi_test_checks", "spi_test_failures", "spi_test_last_failure",
             "spi_test_injected_timeouts", "spi_test_injected_cleanup_errors"]

    with tempfile.TemporaryDirectory(prefix="emelino-spi-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        (output / "machine.repl").write_text(
            f'using "{alias}/stm32f4.repl"\n'
            'nvic:\n    systickFrequency: 16000000\n'
            'flash:\n    size: 0x80000\nsram:\n    size: 0x20000\n'
            'peer: SPI.EmelinoSpiPeer @ spi1\n'
            'gpioPortA:\n    4 -> peer@0\n    3 -> peer@1\n    2 -> peer@2\n    1 -> peer@3\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  f'include @{alias}/SpiPeer.cs', 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  'sysbus SetHookBeforePeripheralWrite sysbus.spi1 "if offset in [0, 12]: self.Log(LogLevel.Info, \'EMELINO_SPI_{0} value={1}\', \'DR\' if offset == 12 else \'CR1\', value)"',
                  f'sysbus SetHookAfterPeripheralRead sysbus.spi1 "if offset == 8 and sysbus.ReadDoubleWord(0x{symbols["spi_test_injection"]:x}) == 3: value = value & ~1"',
                  f'sysbus LoadBinary @{alias}/firmware/firmware.bin 0x08000000',
                  'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset',
                  'emulation RunFor "0:00:00.050"',
                  f'sysbus WriteDoubleWord 0x{symbols["spi_test_command"]:x} 1',
                  'emulation RunFor "0:00:00.005"',
                  f'sysbus WriteDoubleWord 0x{symbols["spi_test_command"]:x} 2',
                  'emulation RunFor "0:00:00.005"']
        for name in names:
            script += [f'echo "SPI_FINAL_{name}"', f'sysbus ReadDoubleWord 0x{symbols[name]:x}']
        for label, address in {"MODER": 0x40020000, "AFRL": 0x40020020,
                               "ODR": 0x40020014, "RCC_APB2ENR": 0x40023844}.items():
            script += [f'echo "SPI_REGISTER_{label}"', f'sysbus ReadDoubleWord 0x{address:x}']
        script.append('quit')
        (output / "run.resc").write_text("\n".join(script) + "\n")
        env = dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"), TMPDIR=temporary)
        status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e",
                           f"include @{alias}/run.resc"], cwd=ROOT, env=env, timeout=args.timeout)
        (output / "renode.log").write_text(log)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    version = re.search(r"Renode, version (\S+)", log)
    report["renode_version"] = version[1] if version else "unknown"
    # This model tags DFF but only exchanges bytes. Every other warning is a failure.
    known = re.compile(r"^.*\[WARNING\] spi1: Unhandled write to offset 0x0\. Unhandled bits: \[11\] when writing value 0x[0-9A-Fa-f]+\. Tags: DFF \(0x1\)\.\s*$", re.M)
    initial_disable = re.compile(r"^.*\[WARNING\] spi1: Slave mode is not supported\s*$", re.M)
    controls = [int(number) for number in re.findall(r"EMELINO_SPI_CR1 value=(\d+)", model_log)]
    if len(initial_disable.findall(model_log)) != 1 or not controls or controls[0] != 0 or controls.count(0) != 1:
        raise ValueError("unexpected SPI master/slave configuration; HAL's initial disabled CR1 write must be the only zero")
    report["known_model_warnings"] = [line.strip() for line in known.findall(model_log) + initial_disable.findall(model_log)]
    combined = initial_disable.sub("", known.sub("", log + model_log))
    if status or re.search(r"\[(?:ERROR|WARNING)\]|There was an error executing command|Unhandled Exception", combined):
        raise ValueError(f"Renode failed or emitted a warning; see {output / 'renode.log'} and model.log")
    metrics = {name: value(log, "SPI_FINAL_" + name) for name in names}
    expected = {"emela_board_completed": 1, "emela_board_failures": 0, "emela_board_boot_initialized": 1,
                "spi_test_stage": 2, "spi_test_checks": 32, "spi_test_failures": 0,
                "spi_test_last_failure": 0, "spi_test_injected_timeouts": 1,
                "spi_test_injected_cleanup_errors": 2}
    if metrics != expected:
        raise ValueError(f"firmware checks failed: {metrics!r}")
    transmitted = [int(number) for number in re.findall(r"EMELINO_SPI tx=(\d+)", model_log)]
    expected_tx = [0, 1, 127, 255, 16, 32, 255, 255, 255,
                   1, 2, 255, 255, 3, 4, 5, 6, 7, 255, 52, 255, 9, 10]
    if transmitted != expected_tx:
        raise ValueError(f"SPI peer byte mismatch: {transmitted!r}")
    word_writes = [int(number) for number in re.findall(r"EMELINO_SPI_DR value=(\d+)", model_log)]
    if word_writes != expected_tx[:-4] + [4660, 65535, 9, 10]:
        raise ValueError(f"SPI DR word mismatch: {word_writes!r}")
    cs = [[int(level) for level in re.findall(rf"EMELINO_SPI_CS pin={pin} level=(\d+)", model_log)]
          for pin in range(4)]
    if cs != [[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1], [0, 1, 0], [0, 1], [0, 1]]:
        raise ValueError(f"chip-select transitions differ: {cs!r}")
    if value(log, "SPI_REGISTER_MODER") & 0xfc00 != 0xa800:
        raise ValueError("SPI bus pins are not in alternate-function mode")
    if value(log, "SPI_REGISTER_AFRL") & 0xfff00000 != 0x55500000:
        raise ValueError("SPI bus pins do not select AF5")
    if value(log, "SPI_REGISTER_ODR") & 0x18 != 0x10:
        raise ValueError("chip selects are not both inactive")
    if not value(log, "SPI_REGISTER_RCC_APB2ENR") & 0x1000:
        raise ValueError("SPI1 clock is disabled")
    report.update({"status": "passed", "metrics": metrics, "transmitted_bytes": transmitted,
                   "data_register_writes": word_writes, "chip_select_transitions": cs})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--renode", help="Renode executable (or RENODE)")
    parser.add_argument("--cc", help="arm-none-eabi-gcc executable (or ARM_GCC)")
    parser.add_argument("--sdk-dir", type=Path, help="STM32CubeF4 SDK root (or STM32CUBE_F4)")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out/stm32f4-spi")
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    report = {"schema_version": 1, "status": "running", "optimization": args.optimization,
              "boot": "raw-flash-reset", "sets_pc_sp": False, "sets_vtor": False}
    try:
        test(args, report)
        print("PASS: 32 SPI checks, mixed transactions, CS cleanup, HAL timeout, 8-bit peer and 16-bit DR writes")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update({"status": "failed", "error": str(error)})
        print(error, file=sys.stderr)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
