#!/usr/bin/env python3
"""Compile an Emela GPIO test and boot its raw Flash image in Renode."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/boards/stm32f4"


def run(command, timeout=60, **kwargs):
    process = subprocess.Popen(list(map(str, command)), stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, errors="replace",
                               start_new_session=True, **kwargs)
    try:
        log, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        log, _ = process.communicate()
        raise ValueError("command timed out: " + str(command[0]) + "\n" + log)
    return process.returncode, log


def value(log, label):
    match = re.search(r"^" + re.escape(label) + r"\s*\n\s*(0x[0-9a-fA-F]+)\s*$", log, re.M)
    if not match:
        raise ValueError("missing Renode metric: " + label)
    return int(match[1], 16)


def test(args, report):
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    emelino = args.emelino.resolve()
    renode = Path(args.renode or os.environ.get("RENODE") or "renode").expanduser()
    found = shutil.which(str(renode))
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    renode = Path(found).resolve()
    generated, firmware = output / "generated", output / "firmware"
    commands = [[emelino, "build", FIXTURE / "main.emel", "--config", FIXTURE / "config.json",
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
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"]})
    source_platform = renode.parent / "platforms/cpus/stm32f4.repl"
    platform = source_platform.read_text()
    report["platform_sha256"] = hashlib.sha256(source_platform.read_bytes()).hexdigest()
    report["model"] = "Renode generic STM32F4; F446RE memory sizes; PA input IRQs"
    report["model_limitations"] = ["not an STM32F446 silicon model",
                                   "SYSCFG register storage only; EXTI port mux is not emulated",
                                   "OTYPER is not implemented; the requested bus write is observed",
                                   "electrical timing and pull resistors are not validated"]
    platform = "\n".join(line for line in platform.splitlines() if "ApplySVD " not in line) + "\n"
    registration = "flash: Memory.MappedMemory @ sysbus 0x08000000"
    if platform.count(registration) != 1:
        raise ValueError("unsupported Renode platform Flash registration")
    platform = platform.replace(registration, "flash: Memory.MappedMemory @ { sysbus 0x08000000; sysbus 0x00000000 }")
    (output / "stm32f4.repl").write_text(platform)
    shutil.copyfile(FIXTURE / "GpioObserver.cs", output / "GpioObserver.cs")

    # Monitor @paths have no shell quoting. A short alias also supports output paths with spaces.
    with tempfile.TemporaryDirectory(prefix="emelino-f4-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        (output / "machine.repl").write_text(
            f'using "{alias}/stm32f4.repl"\n'
            'nvic:\n    systickFrequency: 16000000\n'
            'flash:\n    size: 0x80000\nsram:\n    size: 0x20000\n'
            'syscfg: Memory.MappedMemory @ sysbus 0x40013800\n    size: 0x400\n'
            'observer: Miscellaneous.EmelinoGpioObserver @ sysbus\n'
            'gpioPortA:\n    5 -> observer@0\n    2 -> observer@1\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  f'include @{alias}/GpioObserver.cs', 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  'sysbus SetHookBeforePeripheralWrite sysbus.gpioPortA "if offset == 4: self.Log(LogLevel.Info, \'EMELINO_OTYPER_WRITE value={0}\', value)"',
                  f'sysbus LoadBinary @{alias}/firmware/firmware.bin 0x08000000',
                  'sysbus.cpu PerformanceInMips 100']
        # Poison initialized and zeroed RAM, then boot using the real Flash reset vectors.
        for name in ("emela_board_boot_data", "emela_board_boot_bss"):
            script.append(f'sysbus WriteDoubleWord 0x{symbols[name]:x} 0xdeadbeef')
        script += ['sysbus.cpu Reset', 'emulation RunFor "0:00:00.005"']

        def metric(label, address):
            script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord 0x{address:x}'])

        def snapshot(stage):
            metric(f"GPIO_STAGE_{stage}", symbols["gpio_test_stage"])
            metric(f"GPIO_EVENTS_{stage}", symbols["gpio_test_event_count"])
            metric(f"GPIO_ODR_{stage}", 0x40020014)

        def drive(pin, level):
            script.extend([f'sysbus.gpioPortA OnGPIO {pin} {str(level).lower()}',
                           'emulation RunFor "0:00:00.001"'])

        snapshot(0)
        # Both edges on PA0, rising only on PA1. Separate physical IRQs call separate handlers.
        drive(0, True)
        drive(0, False)
        drive(1, True)
        drive(1, False)
        drive(6, True)
        drive(7, True)
        snapshot(1)
        script += [f'sysbus WriteDoubleWord 0x{symbols["gpio_test_command"]:x} 1',
                   'emulation RunFor "0:00:00.005"']
        snapshot(2)
        # Replacement listens only to falling; disabled PA1 must stay silent.
        drive(0, True)
        drive(0, False)
        drive(1, True)
        drive(6, False)
        drive(6, True)
        drive(7, False)
        drive(7, True)
        snapshot(3)
        script += [f'sysbus WriteDoubleWord 0x{symbols["gpio_test_command"]:x} 2',
                   'emulation RunFor "0:00:00.005"']
        snapshot(4)
        drive(0, True)
        drive(0, False)
        drive(1, False)
        drive(1, True)
        drive(7, False)
        drive(7, True)
        snapshot(5)
        script += [f'sysbus WriteDoubleWord 0x{symbols["gpio_test_command"]:x} 3',
                   'emulation RunFor "0:00:00.005"']
        names = ["emela_board_completed", "emela_board_failures", "emela_board_boot_initialized",
                 "emela_board_boot_data", "emela_board_boot_bss", "gpio_test_checks",
                 "gpio_test_failures", "gpio_test_last_failure", "gpio_test_event_count"]
        for name in names:
            metric("GPIO_FINAL_" + name, symbols[name])
        for index in range(7):
            metric(f"GPIO_EVENT_{index}", symbols["gpio_test_events"] + index * 4)
            metric(f"GPIO_IPSR_{index}", symbols["gpio_test_ipsr"] + index * 4)
        for label, address in {"MODER": 0x40020000, "OTYPER": 0x40020004,
                               "PUPDR": 0x4002000c, "ODR": 0x40020014,
                               "RCC": 0x40023830, "EXTI_IMR": 0x40013c00}.items():
            metric("GPIO_REGISTER_" + label, address)
        script.append('quit')
        (output / "run.resc").write_text("\n".join(script) + "\n")
        env = dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"), TMPDIR=temporary)
        status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e",
                           f"include @{alias}/run.resc"], cwd=ROOT, env=env, timeout=args.timeout)
        (output / "renode.log").write_text(log)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    version = re.search(r"Renode, version (\S+)", log)
    report["renode_version"] = version[1] if version else "unknown"
    # Renode 1.16.1 stores MODER/ODR but intentionally does not model OTYPER.
    known_warning = re.compile(r"^.*\[WARNING\] gpioPortA: Unhandled write to offset 0x4\. Unhandled bits: \[2\] when writing value 0x4\. Tags: OT2 \(0x1\)\.\s*$", re.M)
    combined = known_warning.sub("", log + model_log)
    report["known_model_warnings"] = [line.strip() for line in known_warning.findall(model_log)]
    if status or re.search(r"\[(?:ERROR|WARNING)\]|There was an error executing command|Unhandled Exception", combined):
        raise ValueError(f"Renode failed or emitted a warning; see {output / 'renode.log'} and model.log")
    stages = [(value(log, f"GPIO_STAGE_{i}"), value(log, f"GPIO_EVENTS_{i}"),
               (value(log, f"GPIO_ODR_{i}") >> 5) & 1) for i in range(6)]
    expected_stages = [(1, 0, 1), (1, 5, 0), (2, 5, 0), (2, 7, 0), (3, 7, 0), (3, 7, 0)]
    if stages != expected_stages:
        raise ValueError(f"GPIO stage/event/output mismatch: {stages!r} != {expected_stages!r}")
    metrics = {name: value(log, "GPIO_FINAL_" + name) for name in names}
    expected = {"emela_board_completed": 1, "emela_board_failures": 0,
                "emela_board_boot_initialized": 1, "emela_board_boot_data": 0x454d4244,
                "emela_board_boot_bss": 0x4253534f, "gpio_test_checks": 16,
                "gpio_test_failures": 0, "gpio_test_last_failure": 0, "gpio_test_event_count": 7}
    if metrics != expected:
        raise ValueError(f"firmware checks failed: {metrics!r}")
    events = [value(log, f"GPIO_EVENT_{i}") for i in range(7)]
    ipsr = [value(log, f"GPIO_IPSR_{i}") for i in range(7)]
    if events != [10001, 10000, 20011, 10061, 10071, 30000, 10071] or ipsr != [22, 22, 23, 39, 39, 22, 39]:
        raise ValueError(f"wrong callback/pin/level or not running in EXTI ISR: {events!r}, {ipsr!r}")
    transitions = [int(level) for level in re.findall(r"EMELINO_GPIO_OUTPUT pin=0 level=(\d+)", model_log)]
    if transitions != [1, 0, 1, 0, 1, 0, 1, 0, 1, 0]:
        raise ValueError(f"wrong external GPIO output transitions: {transitions!r}")
    if ((value(log, "GPIO_REGISTER_MODER") >> 10) & 3) != 1 or value(log, "GPIO_REGISTER_EXTI_IMR") & 0xff:
        raise ValueError("incorrect GPIO output mode or interrupts remain enabled")
    if value(log, "GPIO_REGISTER_PUPDR") & 0xf != 0xa or value(log, "GPIO_REGISTER_RCC") & 3 != 3:
        raise ValueError("incorrect input pull configuration or disabled GPIO clock")
    # Output latch and mode only; Renode does not validate open-drain electrical behavior.
    open_drain = [int(level) for level in re.findall(r"EMELINO_GPIO_OUTPUT pin=1 level=(\d+)", model_log)]
    if open_drain != [1, 0, 1]:
        raise ValueError(f"wrong open-drain latch transitions: {open_drain!r}")
    if ((value(log, "GPIO_REGISTER_MODER") >> 4) & 0xf) != 0xd:
        raise ValueError("incorrect open-drain output/analog modes")
    type_writes = [int(number) for number in re.findall(r"EMELINO_OTYPER_WRITE value=(\d+)", model_log)]
    if type_writes != [0, 4] or not value(log, "GPIO_REGISTER_ODR") & 4:
        raise ValueError("open-drain type was not requested or final output latch is incorrect")
    if value(log, "GPIO_REGISTER_PUPDR") & 0xf0:
        raise ValueError("unexpected open-drain/analog pull configuration")
    report.update({"status": "passed", "metrics": metrics, "events": events, "ipsr": ipsr,
                   "output_transitions": transitions, "open_drain_latch_transitions": open_drain,
                   "output_type_bus_writes": type_writes,
                   "stages": stages})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--renode", help="Renode executable (or RENODE)")
    parser.add_argument("--cc", help="arm-none-eabi-gcc executable (or ARM_GCC)")
    parser.add_argument("--sdk-dir", type=Path, help="STM32CubeF4 SDK root (or STM32CUBE_F4)")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out/stm32f4-gpio")
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    report = {"schema_version": 1, "status": "running", "optimization": args.optimization,
              "boot": "raw-flash-reset", "sets_pc_sp": False, "sets_vtor": False}
    try:
        test(args, report)
        print("PASS: 16 GPIO checks, 7 ISR callbacks, 10 output and 3 open-drain latch transitions")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update({"status": "failed", "error": str(error)})
        print(error, file=sys.stderr)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
