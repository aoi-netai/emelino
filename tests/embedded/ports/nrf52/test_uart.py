#!/usr/bin/env python3
"""Exercise UART IRQ ring boundaries and recovery with the real generated firmware."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests/embedded"))
from support.common import load_module, load_target, renode_workspace, run, run_renode, value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cc")
    parser.add_argument("--sdk-dir", type=Path)
    parser.add_argument("--renode", default=os.environ.get("RENODE", "renode"))
    parser.add_argument("--optimization", choices=["Os", "O0"], default="Os")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    output = (args.output_dir or ROOT / "out/embedded/arduino-nano-33-ble" / args.optimization / "uart_regression").resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "uart.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    report = {"status": "running", "optimization": args.optimization, "board": "arduino-nano-33-ble"}
    try:
        emela = '''import Embedded.Board.{BoardUart}
import Embedded.Uart
@external(c, "uart_regression_run")
fn run_test() -> () use {}
@external(c, "uart_regression_failed")
fn failure() -> () use {}
pub fn main() -> () {
  with BoardUart {
    (use Uart.Uart).configure(Uart.Uart0, Uart.UartConfig(115200, Uart.Bits8, Uart.One,
      Uart.NoParity, Uart.Both, Uart.NoFlowControl, Uart.Native))
    run_test()
  } escape { Uart.UartError(_) -> failure() }
}
'''
        (output / "main.emel").write_text(emela)
        (output / "config.json").write_text(json.dumps({"entry": "main", "heap_bytes": 4096,
            "exports": [{"function": "main", "symbol": "uart_regression_main"}]}))
        generated = output / "generated"
        commands = [[ROOT / "target/debug/emelino", "build", output / "main.emel", "--config", output / "config.json", "--out-dir", generated],
                    [sys.executable, ROOT / "tools/build_firmware.py", "--board", "arduino-nano-33-ble",
                     "--generated-dir", generated, "--output-dir", output / "firmware",
                     "--optimization", args.optimization, "--source", Path(__file__).with_name("uart_regression.c")]]
        for flag, selected in (("--cc", args.cc), ("--sdk-dir", args.sdk_dir)):
            if selected:
                commands[1] += [flag, selected]
        for index, command in enumerate(commands):
            status, log = run(command, cwd=ROOT, timeout=120)
            (output / f"build-{index}.log").write_text(log)
            if status:
                raise ValueError(log)
        symbols = json.loads((output / "firmware/symbols.json").read_text())
        manifest = json.loads((output / "firmware/firmware.json").read_text())
        report.update(firmware_sha256=manifest["sha256"]["firmware.bin"], resources=manifest["resources"])
        executable = shutil.which(str(Path(args.renode).expanduser()))
        if not executable:
            raise ValueError("Renode not found")
        renode = Path(executable).resolve()
        target = load_target("arduino-nano-33-ble")
        adapter = load_module(Path(__file__).with_name("renode.py"), "nrf52_uart_platform")
        platform = adapter.Platform(target)
        platform.prepare(renode, output, report)
        with renode_workspace(output) as (alias, temporary):
            (output / "machine.repl").write_text(platform.machine_header(alias))
            script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console', 'mach create',
                      f'machine LoadPlatformDescription @{alias}/machine.repl',
                      f'sysbus.uart0 CreateFileBackend @{alias}/uart.log true', platform.load_binary(alias),
                      'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset', 'emulation RunFor "0:00:00.010"']
            for stage, data in ((1, range(64)), (2, [90] * 65), (3, [0, 255])):
                script += [f'echo "UART_STAGE_{stage}"', f'sysbus ReadDoubleWord {symbols["uart_regression_stage"]:#x}']
                for byte in data:
                    script += [f'sysbus.uart0 WriteChar {byte}', 'emulation RunFor "0:00:00.0002"']
                script += [f'sysbus WriteDoubleWord {symbols["uart_regression_command"]:#x} {stage}',
                           'emulation RunFor "0:00:00.010"']
            expected = {"uart_regression_stage": 4, "uart_regression_checks": 20,
                        "uart_regression_failures": 0, "uart_regression_last_failure": 0,
                        "emela_board_completed": 1, "emela_board_failures": 0}
            for name in expected:
                script += [f'echo "UART_{name}"', f'sysbus ReadDoubleWord {symbols[name]:#x}']
            script.append("quit")
            status, log = run_renode(renode, output, alias, temporary, script, report, 60)
        model = (output / "model.log").read_text(encoding="utf-8-sig")
        if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception", log + model):
            raise ValueError("Renode failed; inspect model.log/renode.log")
        metrics = {name: value(log, "UART_" + name) for name in expected}
        stages = [value(log, f"UART_STAGE_{stage}") for stage in (1, 2, 3)]
        warnings = [line for line in model.splitlines() if "[WARNING]" in line]
        report.update(metrics=metrics, stages=stages, model_limitations=platform.limitations(),
                      known_model_warnings=warnings)
        if any(not platform.known_model_warning(line, {"uart": 0}) for line in warnings):
            raise ValueError("unexpected model warning: " + "\n".join(warnings))
        if metrics != expected or stages != [1, 2, 3]:
            raise ValueError(f"UART checks failed: {metrics}; stages={stages}")
        if (output / "uart.log").read_text(encoding="utf-8-sig") != "R":
            raise ValueError("unexpected UART transmit stream")
        report["status"] = "passed"
        print("PASS: nRF52 UART ring boundaries, overflow recovery, timeout, configuration (20 checks)")
    except (OSError, ValueError, KeyError) as error:
        report.update(status="failed", error=str(error))
        print(str(error), file=sys.stderr)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
