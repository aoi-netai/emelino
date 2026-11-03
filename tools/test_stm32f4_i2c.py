#!/usr/bin/env python3
"""Boot generated Emela I2C firmware with the original STM32F4 Renode controller."""
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
FIXTURE = ROOT / "tests/boards/stm32f4-i2c"


def test(args, report):
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    found = shutil.which(args.renode or os.environ.get("RENODE") or "renode")
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    renode = Path(found).resolve()
    generated, firmware = output / "generated", output / "firmware"
    commands = [[args.emelino.resolve(), "build", FIXTURE / "main.emel", "--config", FIXTURE / "config.json", "--out-dir", generated],
                [sys.executable, ROOT / "boards/stm32/stm32f4/build.py", "--generated-dir", generated,
                 "--output-dir", firmware, "--source", FIXTURE / "probe.c", "--optimization", args.optimization,
                 "--link-arg=-Wl,--wrap=HAL_I2C_Master_Seq_Transmit_IT"]]
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
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"]})
    original = renode.parent / "platforms/cpus/stm32f4.repl"
    report["platform_sha256"] = hashlib.sha256(original.read_bytes()).hexdigest()
    report["model_limitations"] = ["generic STM32F4, not STM32F446 silicon",
        "I2C peer supplies a register window because Renode 1.16.1 requests Read(1) once",
        "one-byte receive is tested as a known model Timeout: ADDR is absent and BTF is unconditional",
        "NACK mapping/retries use a test-only HAL completion wrapper; absent targets do not set AF in this model",
        "I2C POS bit, open-drain drive and external pull resistors are not electrically modeled",
        "recovery tests controller reset and pin sequence, not a physically stuck bus"]
    platform = "\n".join(line for line in original.read_text().splitlines() if "ApplySVD " not in line) + "\n"
    platform = platform.replace("flash: Memory.MappedMemory @ sysbus 0x08000000",
        "flash: Memory.MappedMemory @ { sysbus 0x08000000; sysbus 0x00000000 }")
    (output / "stm32f4.repl").write_text(platform)
    shutil.copyfile(FIXTURE / "I2cPeer.cs", output / "I2cPeer.cs")
    with tempfile.TemporaryDirectory(prefix="emelino-i2c-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        (output / "machine.repl").write_text(f'using "{alias}/stm32f4.repl"\n'
            'nvic:\n    systickFrequency: 16000000\n'
            'flash:\n    size: 0x80000\nsram:\n    size: 0x20000\n'
            'peer: I2C.EmelinoI2cPeer @ i2c1 0x50\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
            f'include @{alias}/I2cPeer.cs', 'mach create',
            f'machine LoadPlatformDescription @{alias}/machine.repl',
            'sysbus SetHookBeforePeripheralWrite sysbus.i2c1 "if offset == 0: self.Log(LogLevel.Info, \'EMELINO_I2C_CR1 value={0}\', value)"',
            f'sysbus.cpu AddHook 0x{symbols["I2C1_EV_IRQHandler"] & ~1:x} "self.Log(LogLevel.Info, \'EMELINO_I2C_EV_IRQ\')"',
            f'sysbus LoadBinary @{alias}/firmware/firmware.bin 0x08000000',
            'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset',
            'sysbus.gpioPortB OnGPIO 8 true', 'sysbus.gpioPortB OnGPIO 9 true',
            'emulation RunFor "0:00:00.050"']

        def metric(label, address):
            script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord 0x{address:x}'])

        metric("I2C_STAGE_1", symbols["i2c_test_stage"])
        script += ['sysbus.i2c1.peer StallReads true', f'sysbus WriteDoubleWord 0x{symbols["i2c_test_command"]:x} 1',
                   'emulation RunFor "0:00:00.010"']
        metric("I2C_STAGE_2", symbols["i2c_test_stage"])
        script += ['sysbus.i2c1.peer StallReads false', f'sysbus WriteDoubleWord 0x{symbols["i2c_test_command"]:x} 2',
                   'emulation RunFor "0:00:00.010"']
        metric("I2C_STAGE_3", symbols["i2c_test_stage"])
        script += [f'sysbus WriteDoubleWord 0x{symbols["i2c_test_command"]:x} 3',
                   'emulation RunFor "0:00:00.005"']
        names = ["emela_board_completed", "emela_board_failures", "emela_board_boot_initialized",
                 "i2c_test_checks", "i2c_test_failures", "i2c_test_last_failure", "i2c_test_injected_nacks"]
        for name in names:
            metric("I2C_FINAL_" + name, symbols[name])
        for label, address in {"CCR": 0x4000541c, "CR1": 0x40005400, "MODER": 0x40020400,
                               "AFRH": 0x40020424, "RCC": 0x40023840}.items():
            metric("I2C_REGISTER_" + label, address)
        script.append('quit')
        (output / "run.resc").write_text("\n".join(script) + "\n")
        env = dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"), TMPDIR=temporary)
        status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e", f"include @{alias}/run.resc"],
                          cwd=ROOT, env=env, timeout=args.timeout)
        (output / "renode.log").write_text(log)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    version = re.search(r"Renode, version (\S+)", log)
    report["renode_version"] = version[1] if version else "unknown"
    if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception", log + model_log):
        raise ValueError("Renode failed; see renode.log/model.log")
    warnings = [line for line in model_log.splitlines() if "[WARNING]" in line]
    known_gpio = r"\[WARNING\] gpioPortB: Unhandled write to offset 0x4\. Unhandled bits: \[(8|9)\] when writing value 0x(100|200)\. Tags: OT(8|9) \(0x1\)\."
    known_fifo = "[WARNING] i2c1: Tried to read from an empty fifo"
    unknown = [line for line in warnings if not re.search(known_gpio, line) and not line.endswith(known_fifo)]
    if unknown:
        raise ValueError("unexpected Renode warning: " + "\n".join(unknown))
    report["expected_model_warnings"] = warnings
    metrics = {name: value(log, "I2C_FINAL_" + name) for name in names}
    stages = [value(log, f"I2C_STAGE_{i}") for i in range(1, 4)]
    expected = {"emela_board_completed": 1, "emela_board_failures": 0, "emela_board_boot_initialized": 1,
                "i2c_test_checks": 20, "i2c_test_failures": 0, "i2c_test_last_failure": 0,
                "i2c_test_injected_nacks": 3}
    report.update({"metrics": metrics, "stages": stages})
    if metrics != expected or stages != [1, 2, 3]:
        raise ValueError(f"I2C checks failed: {metrics}; stages={stages}")
    writes = re.findall(r"EMELINO_I2C_WRITE ([0-9A-F-]*)", model_log)
    for packet in ["10-AA-BB", "A0-B1-02", "20-44-55", "01-30-66-77"]:
        if packet not in writes:
            raise ValueError("missing I2C wire packet " + packet)
    irq_count = model_log.count("EMELINO_I2C_EV_IRQ")
    if not irq_count:
        raise ValueError("interrupt-enabled I2C transfer did not enter hardware IRQ")
    if value(log, "I2C_REGISTER_CCR") & 0xfff != 80 or value(log, "I2C_REGISTER_MODER") >> 16 & 0xf != 0xa:
        raise ValueError("I2C recovery did not restore clock/pin configuration")
    transactions, current = [], []
    for item in re.findall(r"EMELINO_I2C_CR1 value=(\d+)", model_log):
        bits = int(item)
        if bits & 0x100:
            current.append("START")
        if bits & 0x200:
            current.append("STOP")
            transactions.append(current)
            current = []
    single, combined = ["START", "STOP"], ["START", "START", "STOP"]
    expected_transactions = [single, combined, single, single, combined, single, combined,
                             single, combined, single, combined, combined, single, combined, combined]
    if current or transactions != expected_transactions:
        raise ValueError("unexpected START/STOP sequence: " + repr(transactions))
    report.update({"status": "passed", "writes": writes, "event_irq_log_entries": irq_count,
                   "start_stop_transactions": transactions,
                   "successful_behavior_checks": 19, "known_model_limit_checks": 1})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--renode")
    parser.add_argument("--cc")
    parser.add_argument("--sdk-dir", type=Path)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "out/stm32f4-i2c")
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    report = {"schema_version": 1, "status": "running", "boot": "raw-flash-reset", "optimization": args.optimization}
    try:
        test(args, report)
        print("PASS: 19 I2C checks plus 1 known model limit; polling/IRQ, repeated START, timeout and recovery")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update({"status": "failed", "error": str(error)})
        print(error, file=sys.stderr)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
