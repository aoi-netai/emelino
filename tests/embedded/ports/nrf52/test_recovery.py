#!/usr/bin/env python3
"""Inject lost controller completion events, then recover against real device models."""
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


def fault_hook(peripheral, label, twi=False):
    # Renode 1.16 has no after-write hook. Drop the first pending READY/RXREADY
    # on its first CPU read, after the real model has exchanged the byte.
    # self.WriteDoubleWord addresses the controller directly, bypassing bus hooks.
    clear = "self.WriteDoubleWord(264, 0); "
    if twi:
        clear += "self.WriteDoubleWord(260, 0); "  # suppress the matching STOPPED
    code = (f"if offset == 264 and value and not globals().get('lost_{label}', False): "
            f"globals()['lost_{label}'] = True; {clear}value = 0; "
            f"self.Log(LogLevel.Info, 'RECOVERY_FAULT_{label}')")
    return f'sysbus SetHookAfterPeripheralRead sysbus.{peripheral} "{code}"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cc")
    parser.add_argument("--sdk-dir", type=Path)
    parser.add_argument("--renode", default=os.environ.get("RENODE", "renode"))
    parser.add_argument("--optimization", choices=["Os", "O0"], default="Os")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    output = (args.output_dir or ROOT / "out/embedded/arduino-nano-33-ble" / args.optimization / "recovery_regression").resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    report = {"status": "running", "optimization": args.optimization, "board": "arduino-nano-33-ble",
              "bsp_wrappers": False, "device_response_injection": False,
              "controller_event_fault_injection": True,
              "faults": ["SPI: clear first pending READY event and return zero on its first read",
                         "TWI polling and IRQ: clear first pending RXREADY and STOPPED events and return zero on RXREADY read"],
              "recovery_hook_policy": "replace each fault hook with a no-op before calling recover",
              "models": ["Renode built-in Micron_MT25Q", "Renode built-in BMP180"]}
    try:
        emela = '''import Embedded.Board.{BoardGpio, BoardUart, BoardSpi, BoardI2c}
import Embedded.Gpio
import Embedded.Uart
import Embedded.Spi
import Embedded.I2c
@external(c, "recovery_regression_run")
fn run_test() -> () use {}
@external(c, "recovery_regression_failed")
fn failure() -> () use {}
pub fn main() -> () {
  with BoardGpio { with BoardUart { with BoardSpi { with BoardI2c {
    (use Gpio.Gpio).configure(Gpio.GpioPin(16), Gpio.OutputPushPull(Gpio.NoPull, Gpio.High))
    (use Uart.Uart).configure(Uart.Uart0, Uart.UartConfig(115200, Uart.Bits8, Uart.One,
      Uart.NoParity, Uart.Both, Uart.NoFlowControl, Uart.Native))
    (use Spi.Spi).configure(Spi.Spi1, Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8,
      Spi.IdleLow, Spi.FirstEdge, 1000000, Spi.MostSignificantFirst))
    (use I2c.I2c).configure(I2c.I2c1, I2c.I2cConfig(100000, I2c.Standard))
    run_test()
  } } } } escape {
    Gpio.GpioError(_) -> failure()
    Uart.UartError(_) -> failure()
    Spi.SpiError(_) -> failure()
    I2c.I2cError(_) -> failure()
  }
}
'''
        (output / "main.emel").write_text(emela)
        (output / "config.json").write_text(json.dumps({"entry": "main", "heap_bytes": 4096,
            "exports": [{"function": "main", "symbol": "recovery_regression_main"}]}))
        generated = output / "generated"
        commands = [[ROOT / "target/debug/emelino", "build", output / "main.emel", "--config", output / "config.json", "--out-dir", generated],
                    [sys.executable, ROOT / "tools/build_firmware.py", "--board", "arduino-nano-33-ble",
                     "--generated-dir", generated, "--output-dir", output / "firmware",
                     "--optimization", args.optimization, "--source", Path(__file__).with_name("recovery_regression.c")]]
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
        adapter = load_module(Path(__file__).with_name("renode.py"), "nrf52_recovery_platform")
        platform = adapter.Platform(target)
        settings = {"uart": 0, "spi": 1, "i2c": 1, "cs_pin": 34}
        platform.prepare(renode, output, report, ROOT / "tests/renode/models/EmelinoNRF52840I2C.cs", 1)
        with renode_workspace(output) as (alias, temporary):
            (output / "machine.repl").write_text(platform.machine_header(alias) +
                'bmp180: Sensors.BMP180 @ twi0 0x77\n'
                'flashStorage: Memory.MappedMemory @ sysbus 0x90000000\n    size: 0x800000\n'
                'serialFlash: SPI.Micron_MT25Q @ spi2\n    underlyingMemory: flashStorage\n' +
                platform.mt25q_connection(settings))
            script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                      *platform.controller_includes(alias), 'mach create',
                      f'machine LoadPlatformDescription @{alias}/machine.repl',
                      *platform.bmp180_observe(symbols, settings), *platform.mt25q_observe(settings),
                      platform.load_binary(alias), 'sysbus.cpu PerformanceInMips 100',
                      'sysbus.cpu Reset', 'emulation RunFor "0:00:00.010"']

            def metric(label, address):
                script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord {address:#x}'])

            for stage, peripheral, label in ((1, "spi2", "SPI"), (2, "spi2", None),
                                              (3, "twi0", "TWI_POLL"), (4, "twi0", None),
                                              (5, "twi0", "TWI_IRQ"), (6, "twi0", None)):
                metric(f"RECOVERY_STAGE_{stage}", symbols["recovery_stage"])
                if stage == 2:
                    metric("RECOVERY_SPI_TIMEOUT_CS", 0x50000804)
                script.append(fault_hook(peripheral, label, peripheral == "twi0") if label else
                              f'sysbus SetHookAfterPeripheralRead sysbus.{peripheral} "pass"')
                script += [f'sysbus WriteDoubleWord {symbols["recovery_command"]:#x} {stage}',
                           'emulation RunFor "0:00:00.050"']
            expected = {"recovery_stage": 7, "recovery_checks": 22, "recovery_failures": 0,
                        "recovery_last_failure": 0, "recovery_spi_timeout": 5,
                        "recovery_spi_blocked": 8, "recovery_twi_poll_timeout": 5,
                        "recovery_twi_irq_timeout": 5, "recovery_spi_recover_status": 0,
                        "recovery_spi_read_status": 0, "recovery_jedec": 0x20bb17,
                        "recovery_poll_sensor_id": 0x55, "recovery_irq_sensor_id": 0x55,
                        "emela_board_completed": 1,
                        "emela_board_failures": 0, "emela_board_boot_initialized": 1}
            measured = [*expected, "recovery_spi_elapsed", "recovery_twi_poll_elapsed", "recovery_twi_irq_elapsed"]
            for name in measured:
                metric("RECOVERY_" + name, symbols[name])
            script.append("quit")
            status, log = run_renode(renode, output, alias, temporary, script, report, 60)
        model = (output / "model.log").read_text(encoding="utf-8-sig")
        if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception|Fatal error", log + model):
            raise ValueError("Renode failed; inspect model.log/renode.log")
        metrics = {name: value(log, "RECOVERY_" + name) for name in measured}
        stages = [value(log, f"RECOVERY_STAGE_{stage}") for stage in range(1, 7)]
        faults = re.findall(r"RECOVERY_FAULT_(SPI|TWI_POLL|TWI_IRQ)", model)
        report.update(metrics=metrics, stages=stages, injected_faults=faults,
                      model_limitations=platform.limitations())
        warnings, started = [], False
        for line in model.splitlines():
            if "FLASH_TX " in line:
                started = True
            if "[WARNING]" in line:
                initial = not started and line.endswith("[WARNING] spi2.serialFlash: Transmission finished in the unexpected state: RecognizeOperation")
                if not initial and not platform.known_model_warning(line, settings):
                    raise ValueError("unexpected model warning: " + line)
                warnings.append(line)
        report["known_model_warnings"] = warnings
        if any(metrics[name] != expected[name] for name in expected) or stages != list(range(1, 7)):
            raise ValueError(f"recovery checks failed: {metrics}; stages={stages}")
        if faults != ["SPI", "TWI_POLL", "TWI_IRQ"]:
            raise ValueError("fault hooks did not fire exactly once each: " + repr(faults))
        if not value(log, "RECOVERY_SPI_TIMEOUT_CS") & 4:
            raise ValueError("SPI timeout left chip select asserted")
        frames, frame = [], None
        for event, number in re.findall(r"FLASH_(CS|TX) (\d+)", model):
            number = int(number)
            if event == "CS":
                if number == 0:
                    if frame is not None:
                        raise ValueError("nested chip select assertion")
                    frame = []
                elif frame is not None:
                    frames.append(frame)
                    frame = None
            elif frame is None:
                raise ValueError("SPI byte sent with inactive chip select")
            else:
                frame.append(number)
        if frame is not None or frames != [[0x9f], [0x9f, 0xff, 0xff, 0xff]]:
            raise ValueError("timeout/recovered command frames mismatch: " + repr(frames))
        report["command_frames"] = frames
        report.update(platform.validate_mt25q_configuration(model, settings))
        report["status"] = "passed"
        print("PASS: SPI and TWI polling/IRQ event-loss timeout, cleanup, recovery and real-device reads (22 checks)")
    except (OSError, ValueError, KeyError) as error:
        report.update(status="failed", error=str(error))
        print(str(error), file=sys.stderr)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
