"""All four services sharing one real generated application and real device models."""
import re
from support.common import ROOT, build_firmware, renode_workspace, run_renode, value


def test(args, report):
    output, renode, _, symbols = build_firmware(args, report, "peripheral_contract")
    settings = args.target["scenarios"]["peripheral_contract"]
    controller = ROOT / settings["controller_model"]
    args.platform.prepare(renode, output, report, controller, settings["i2c"])
    report.update(model_limitations=args.platform.limitations(), bsp_wrappers=False,
                  device_response_injection=False)
    expected = {"emela_board_completed": 1, "emela_board_failures": 0,
                "emela_board_boot_initialized": 1, "contract_checks": 24,
                "contract_failures": 0, "contract_last_failure": 0,
                "contract_isr_calls": 1, "contract_isr_ipsr": 22,
                "contract_context_failures": 0}
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(args.platform.machine_header(alias) +
            f'bmp180: Sensors.BMP180 @ twi0 {settings["address"]:#x}\n'
            'flashStorage: Memory.MappedMemory @ sysbus 0x90000000\n    size: 0x800000\n'
            'serialFlash: SPI.Micron_MT25Q @ spi2\n    underlyingMemory: flashStorage\n' +
            args.platform.mt25q_connection(settings))
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  *args.platform.controller_includes(alias), 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  f'sysbus.uart0 CreateFileBackend @{alias}/uart.log true',
                  *args.platform.bmp180_observe(symbols, settings),
                  *args.platform.mt25q_observe(settings),
                  args.platform.load_binary(alias), 'sysbus.cpu PerformanceInMips 100',
                  'sysbus.cpu Reset', 'emulation RunFor "0:00:00.050"']

        def metric(label, address):
            script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord {address:#x}'])

        def advance(stage, words):
            metric(f"CONTRACT_STAGE_{stage}", symbols["contract_stage"])
            for word in words:
                script.extend([f'sysbus.uart0 WriteChar {word}', 'emulation RunFor "0:00:00.001"'])
            script.extend([f'sysbus WriteDoubleWord {symbols["contract_command"]:#x} {stage}',
                           'emulation RunFor "0:00:00.050"'])

        script += [args.platform.drive(settings["pins"]["input0"], True), 'emulation RunFor "0:00:00.001"']
        advance(1, [65, 66])
        advance(2, [67, 68, 69])
        advance(3, [70, 71])
        for name in expected:
            metric("CONTRACT_FINAL_" + name, symbols[name])
        metric("CONTRACT_UART_ENABLE", 0x40002500)
        script.append('quit')
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model = (output / "model.log").read_text(encoding="utf-8-sig")
    if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception", log + model):
        raise ValueError("Renode peripheral contract test failed; see renode.log/model.log")
    warnings, unknown, started = [], [], False
    allowed = "[WARNING] spi2.serialFlash: Transmission finished in the unexpected state: RecognizeOperation"
    for line in model.splitlines():
        if "FLASH_TX " in line:
            started = True
        if "[WARNING]" in line:
            warnings.append(line)
            if not (not started and line.endswith(allowed)) and not args.platform.known_model_warning(line, settings):
                unknown.append(line)
    if unknown:
        raise ValueError("unexpected model warning: " + "\n".join(unknown))
    report.update(args.platform.validate_mt25q_configuration(model, settings))
    metrics = {name: value(log, "CONTRACT_FINAL_" + name) for name in expected}
    stages = [value(log, f"CONTRACT_STAGE_{i}") for i in (1, 2, 3)]
    if metrics != expected or stages != [1, 2, 3]:
        raise ValueError(f"peripheral contract checks failed: {metrics}; stages={stages}")
    uart = (output / "uart.log").read_text(encoding="utf-8-sig")
    if uart != "BOOT\nIRQ\nOK\n":
        raise ValueError("unexpected UART output: " + repr(uart))
    if value(log, "CONTRACT_UART_ENABLE") != 4:
        raise ValueError("UART controller was not in legacy mode")
    report.update(status="passed", metrics=metrics, stages=stages, uart=uart,
                  event_irq_log_entries=model.count("BMP180_EVENT_IRQ"), known_model_warnings=warnings)
