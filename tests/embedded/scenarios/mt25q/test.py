"""Scenario steps and assertions; execution is provided by the common runner."""
import re
from support.common import build_firmware, renode_workspace, run_renode, value


def test(args, report):
    output, renode, manifest, symbols = build_firmware(args, report, "mt25q")
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "model": "Renode built-in Micron_MT25Q, 8 MiB",
                   "model_limitations": [*args.platform.limitations(),
                       "SPI exchanges bytes; CPOL/CPHA/frequency are not electrical timing checks",
                       "flash program/erase completes immediately; busy duration and endurance are not modeled",
                       "tests stay inside one page and do not claim physical NOR programming constraints"],
                   "hal_or_bsp_wrappers": False, "device_response_injection": False})
    settings = args.target["scenarios"]["mt25q"]
    bus = args.platform.mt25q_bus(settings)
    args.platform.prepare(renode, output, report)
    names = ["emela_board_completed", "emela_board_failures", "emela_board_last_defect", "emela_board_boot_initialized",
             "spi_device_test_checks", "spi_device_test_failures", "spi_device_test_last_failure"]
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(
            args.platform.machine_header(alias) +
            'flashStorage: Memory.MappedMemory @ sysbus 0x90000000\n    size: 0x800000\n'
            f'serialFlash: SPI.Micron_MT25Q @ {bus}\n    underlyingMemory: flashStorage\n'
            + args.platform.mt25q_connection(settings))
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  'mach create', f'machine LoadPlatformDescription @{alias}/machine.repl',
                  *args.platform.mt25q_observe(settings),
                  args.platform.load_binary(alias),
                  'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset',
                  'emulation RunFor "0:00:00.050"']

        def metric(label, command):
            script.extend([f'echo "{label}"', command])

        metric("FLASH_STAGE_1", f'sysbus ReadDoubleWord 0x{symbols["spi_device_test_stage"]:x}')
        metric("FLASH_AFTER_PROGRAM", 'sysbus ReadDoubleWord 0x90000020')
        script += [f'sysbus WriteDoubleWord 0x{symbols["spi_device_test_command"]:x} 1',
                   'emulation RunFor "0:00:00.050"']
        metric("FLASH_STAGE_2", f'sysbus ReadDoubleWord 0x{symbols["spi_device_test_stage"]:x}')
        metric("FLASH_AFTER_ERASE", 'sysbus ReadDoubleWord 0x90000020')
        script += [f'sysbus WriteDoubleWord 0x{symbols["spi_device_test_command"]:x} 2',
                   'emulation RunFor "0:00:00.005"']
        for name in names:
            metric("FLASH_FINAL_" + name, f'sysbus ReadDoubleWord 0x{symbols[name]:x}')
        metric("FLASH_FINAL_ODR", args.platform.mt25q_cs_read(settings))
        script.append('quit')
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model = (output / "model.log").read_text(encoding="utf-8-sig")
    if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception", log + model):
        raise ValueError("Renode failed; see renode.log/model.log")
    # GPIO initially deasserts CS with no transaction. GenericSpiFlash logs that
    # empty initial boundary; it is allowed only before the first transmitted byte.
    known, started = [], False
    for line in model.splitlines():
        if "FLASH_TX " in line:
            started = True
        if "[WARNING]" not in line:
            continue
        port_warning = getattr(args.platform, "known_model_warning", lambda line, settings: False)
        allowed = port_warning(line, settings) or (not started and any(line.endswith(text) for text in (
            f"[WARNING] {bus}: Slave mode is not supported",
            f"[WARNING] {bus}.serialFlash: Transmission finished in the unexpected state: RecognizeOperation")))
        if not allowed:
            raise ValueError("unexpected model warning: " + line)
        known.append(line)
    metrics = {name: value(log, "FLASH_FINAL_" + name) for name in names}
    validate_configuration = getattr(args.platform, "validate_mt25q_configuration", None)
    if validate_configuration:
        report.update(validate_configuration(model, settings))
    expected = {"emela_board_completed": 1, "emela_board_failures": 0, "emela_board_last_defect": 0,
                "emela_board_boot_initialized": 1, "spi_device_test_checks": 14,
                "spi_device_test_failures": 0, "spi_device_test_last_failure": 0}
    if metrics != expected or [value(log, f"FLASH_STAGE_{i}") for i in (1, 2)] != [1, 2]:
        raise ValueError(f"firmware checks failed: {metrics}")
    if value(log, "FLASH_AFTER_PROGRAM") != 0xffaa5500 or value(log, "FLASH_AFTER_ERASE") != 0xffffffff:
        raise ValueError("independent flash storage check failed")
    if not args.platform.mt25q_cs_inactive(value(log, "FLASH_FINAL_ODR"), settings):
        raise ValueError("flash chip select was left asserted")
    frames, current, cs = [], None, []
    for event, number in re.findall(r"FLASH_(CS|TX) (\d+)", model):
        number = int(number)
        if event == "CS":
            cs.append(number)
            if number == 0:
                if current is not None:
                    raise ValueError("nested chip select assertion")
                current = []
            elif current is not None:
                frames.append(current)
                current = None
        elif current is None:
            raise ValueError("SPI transfer with chip select inactive")
        else:
            current.append(number)
    read = [3, 0, 0, 32, 255, 255, 255, 255]
    status_command, identify = [5, 255], [159, 255, 255, 255]
    expected_frames = [identify, read, status_command, [6], status_command,
        [2, 0, 0, 32, 0, 85, 170, 255], status_command, read,
        [11, 0, 0, 32, 0, 255, 255, 255, 255], read, [159, 255], identify,
        [6], [4], status_command, [6], [32, 0, 0, 0], read, status_command]
    if current is not None or frames != expected_frames or cs != [1] + [0, 1] * len(expected_frames):
        raise ValueError(f"command/CS trace mismatch: {frames}, CS {cs}")
    report.update({"status": "passed", "metrics": metrics, "command_frames": frames,
                   "chip_select_writes": cs, "after_program": "00 55 AA FF", "after_erase": "FF FF FF FF",
                   "known_model_warnings": known})
