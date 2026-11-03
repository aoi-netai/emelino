"""Scenario steps and assertions; execution is provided by the common runner."""
import json
import re
from support.common import build_firmware, renode_workspace, run_renode, value


def test(args, report):
    output, renode, manifest, symbols = build_firmware(args, report, "bmp180")
    if any(name.startswith("__wrap_HAL_") for name in symbols):
        raise ValueError("sensor test must use the real HAL without test wrappers")
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "sensor_model": "Renode built-in Sensors.BMP180",
                   "hal_wrappers": False, "register_value_overrides": False})
    if args.baseline_report:
        baseline = json.loads(args.baseline_report.read_text())
        same_firmware = baseline.get("firmware_sha256") == report["firmware_sha256"]
        report["baseline_comparison"] = {"report": str(args.baseline_report),
            "status": baseline.get("status"), "identical_firmware": same_firmware,
            "metrics": baseline.get("metrics")}
        if not same_firmware:
            raise ValueError("baseline and updated-controller runs must use identical firmware bytes")
    settings = args.target["scenarios"]["bmp180"]
    bus = args.platform.bmp180_bus(settings)
    args.platform.prepare(renode, output, report, args.controller_model, settings["i2c"])
    report["model_limitations"] = [*args.platform.limitations(),
        "BMP180 conversion is instantaneous in Renode; conversion duration is not tested",
        "temperature is checked with forward compensation; pressure tests cover raw values and oversampling packing",
        "NACK tests use an unregistered bus address, without HAL/error injection"]
    metrics_names = ["emela_board_completed", "emela_board_failures", "emela_board_boot_initialized",
                     "bmp180_test_stage", "bmp180_test_checks", "bmp180_test_failures",
                     "bmp180_test_current", "bmp180_test_last_failure", "bmp180_test_error"]
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(
            args.platform.machine_header(alias) +
            f'bmp180: Sensors.BMP180 @ {bus} {settings["address"]:#x}\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console']
        if args.controller_model:
            script.append(f'include @{alias}/EmelinoSTM32F4I2C.cs')
        script += ['mach create', f'machine LoadPlatformDescription @{alias}/machine.repl',
                   f'logLevel -1 sysbus.{bus}.bmp180',
                   f'sysbus.{bus}.bmp180 Temperature 24', f'sysbus.{bus}.bmp180 UncompensatedPressure 1100',
                   *args.platform.bmp180_observe(symbols, settings),
                   args.platform.load_binary(alias),
                   'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset',
                   'emulation RunFor "0:00:00.100"']

        def metric(label, address):
            script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord 0x{address:x}'])

        metric("BMP180_STAGE_1", symbols["bmp180_test_stage"])
        script += [f'sysbus.{bus}.bmp180 Temperature 32', f'sysbus.{bus}.bmp180 UncompensatedPressure 2200',
                   f'sysbus WriteDoubleWord 0x{symbols["bmp180_test_command"]:x} 1',
                   'emulation RunFor "0:00:00.100"']
        metric("BMP180_STAGE_2", symbols["bmp180_test_stage"])
        script += [f'sysbus WriteDoubleWord 0x{symbols["bmp180_test_command"]:x} 2',
                   'emulation RunFor "0:00:00.005"']
        for name in metrics_names:
            metric("BMP180_FINAL_" + name, symbols[name])
        for slot in range(2):
            metric(f"BMP180_TEMPERATURE_{slot}", symbols["bmp180_test_temperature"] + slot * 4)
        script.append('quit')
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    if status or re.search(r"\[ERROR\]|There was an error executing command|Unhandled Exception|Errors during compilation", log + model_log):
        raise ValueError("Renode failed; see renode.log/model.log")
    metrics = {name: value(log, "BMP180_FINAL_" + name) for name in metrics_names}
    stages = [value(log, f"BMP180_STAGE_{i}") for i in range(1, 3)]
    temperatures = [value(log, f"BMP180_TEMPERATURE_{i}") for i in range(2)]
    irq_count = model_log.count("BMP180_EVENT_IRQ")
    transactions, current = args.platform.bmp180_transactions(model_log)
    report.update({"metrics": metrics, "stages": stages, "temperature_decicelsius": temperatures,
                   "event_irq_log_entries": irq_count, "start_stop_transactions": transactions})
    expected = {"emela_board_completed": 1, "emela_board_failures": 0, "emela_board_boot_initialized": 1,
                "bmp180_test_stage": 2, "bmp180_test_checks": 19, "bmp180_test_failures": 0,
                "bmp180_test_current": 19, "bmp180_test_last_failure": 0, "bmp180_test_error": 0}
    if metrics != expected or stages != [1, 2] or temperatures != [240, 320] or not irq_count:
        raise ValueError(f"BMP180 firmware checks failed: {metrics}; stages={stages}; temperatures={temperatures}")
    # Each prefixed read has exactly one repeated START and one final STOP.
    # Address-only probes, writes and the two standalone reads use one START.
    expected_starts = [2, 1, 2, 2, 1, 1, 1, 1, 2, 2, 1, 2, 1, 2, 2,
                       2, 2, 1, 2, 1, 2, 2, 1, 1, 1, 1, 2, 2, 2, 2]
    if current or transactions != [["START"] * count + ["STOP"] for count in expected_starts]:
        raise ValueError("unexpected BMP180 START/STOP sequence: " + repr(transactions))
    if args.controller_model:
        report["nack_count"] = model_log.count(f'EMELINO_I2C_NACK address={settings["missing_address"]}')
        if report["nack_count"] != 4:
            raise ValueError("missing-address read and three readiness probes must produce four NACKs")
    report["known_model_warnings"] = args.platform.bmp180_warnings(model_log, settings)
    report["status"] = "passed"
