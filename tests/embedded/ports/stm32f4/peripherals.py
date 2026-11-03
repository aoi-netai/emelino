"""Scenario steps and assertions; execution is provided by the common runner."""
import re
from support.common import build_firmware, renode_workspace, run_renode, value


def test(args, report):
    output, renode, manifest, symbols = build_firmware(args, report, "peripherals")
    report.update({"board": manifest["board"], "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "resources": manifest["resources"], "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "model": "Renode generic STM32F4, F446RE memory sizes",
                   "model_limitations": ["configuration and pin ownership only; individual driver suites test data transfers",
                                         "SYSCFG is register storage; GPIO OTYPER and UART reserved SR bits are not modeled",
                                         "not STM32F446 silicon or electrical validation"]})
    args.platform.prepare(renode, output, report)
    expected = {"emela_board_completed": 1, "emela_board_failures": 0,
                "emela_board_boot_initialized": 1, "peripherals_test_checks": 9,
                "peripherals_test_failures": 0, "peripherals_test_last_failure": 0}
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(
            args.platform.machine_header(alias) +
            'syscfg: Memory.MappedMemory @ sysbus 0x40013800\n    size: 0x400\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console', 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  'sysbus SetHookBeforePeripheralWrite sysbus.usart2 "if offset == 4: self.Log(LogLevel.Info, \'EMELINO_TX {0}\', value)"',
                  args.platform.load_binary(alias),
                  'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset', 'emulation RunFor "0:00:00.100"']
        for name in expected:
            script += [f'echo "FINAL_{name}"', f'sysbus ReadDoubleWord 0x{symbols[name]:x}']
        script += ['echo "FINAL_ODR"', 'sysbus ReadDoubleWord 0x40020014', 'quit']
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
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
