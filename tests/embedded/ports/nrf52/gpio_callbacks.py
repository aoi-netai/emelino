"""Exercise per-pin callbacks with the target's shared GPIOTE interrupt."""
import re
from support.common import build_firmware, renode_workspace, run_renode, value


def test(args, report):
    output, renode, _, symbols = build_firmware(args, report, "gpio_callbacks")
    args.platform.prepare(renode, output, report)
    pins = args.target["scenarios"]["gpio_callbacks"]["pins"]
    report["model_limitations"] = args.platform.limitations()
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(args.platform.machine_header(alias))
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  'mach create', f'machine LoadPlatformDescription @{alias}/machine.repl',
                  args.platform.load_binary(alias), 'sysbus.cpu PerformanceInMips 100']
        for name in ("emela_board_boot_data", "emela_board_boot_bss"):
            script.append(f'sysbus WriteDoubleWord {symbols[name]:#x} 0xdeadbeef')
        script += ['sysbus.cpu Reset', 'emulation RunFor "0:00:00.010"']

        def metric(label, address):
            script.extend([f'echo "{label}"', f'sysbus ReadDoubleWord {address:#x}'])

        def stage(number):
            metric(f"CALLBACK_STAGE_{number}", symbols["callbacks_stage"])
            metric(f"CALLBACK_COUNT_{number}", symbols["callbacks_event_count"])

        def drive(name, level):
            script.extend([args.platform.drive(pins[name], level), 'emulation RunFor "0:00:00.001"'])

        stage(0)
        for name, level in (("input0", True), ("input0", False), ("input1", True),
                            ("input1", False), ("input2", True), ("input7", True)):
            drive(name, level)
        stage(1)
        script += [f'sysbus WriteDoubleWord {symbols["callbacks_command"]:#x} 1', 'emulation RunFor "0:00:00.005"']
        stage(2)
        for name, level in (("input0", True), ("input0", False), ("input1", True),
                            ("input8", True), ("input2", False), ("input2", True)):
            drive(name, level)
        stage(3)
        script += [f'sysbus WriteDoubleWord {symbols["callbacks_command"]:#x} 2', 'emulation RunFor "0:00:00.005"']
        stage(4)
        for name in ("input0", "input1", "input2", "input7", "input8"):
            drive(name, False)
            drive(name, True)
        stage(5)
        script += [f'sysbus WriteDoubleWord {symbols["callbacks_command"]:#x} 3', 'emulation RunFor "0:00:00.005"']
        expected = {"emela_board_completed": 1, "emela_board_failures": 0,
                    "emela_board_boot_initialized": 1, "emela_board_boot_data": 0x454d4244,
                    "emela_board_boot_bss": 0x4253534f, "callbacks_checks": 10,
                    "callbacks_failures": 0, "callbacks_last_failure": 0, "callbacks_event_count": 8}
        for name in expected:
            metric("CALLBACK_FINAL_" + name, symbols[name])
        for index in range(8):
            metric(f"CALLBACK_EVENT_{index}", symbols["callbacks_events"] + 4 * index)
            metric(f"CALLBACK_IPSR_{index}", symbols["callbacks_ipsr"] + 4 * index)
        metric("CALLBACK_INTENSET", 0x40006304)
        script.append('quit')
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model = (output / "model.log").read_text(encoding="utf-8-sig")
    known = [line for line in model.splitlines() if "[WARNING]" in line and args.platform.known_model_warning(line, {})]
    unexpected = "\n".join(line for line in (log + model).splitlines() if not args.platform.known_model_warning(line, {}))
    if status or re.search(r"\[(?:ERROR|WARNING)\]|There was an error executing command|Unhandled Exception", unexpected):
        raise ValueError("Renode GPIO callback test failed; see renode.log/model.log")
    metrics = {name: value(log, "CALLBACK_FINAL_" + name) for name in expected}
    stages = [(value(log, f"CALLBACK_STAGE_{i}"), value(log, f"CALLBACK_COUNT_{i}")) for i in range(6)]
    if metrics != expected or stages != [(1, 0), (1, 5), (2, 5), (2, 8), (3, 8), (3, 8)]:
        raise ValueError(f"GPIO callback checks failed: {metrics}; stages={stages}")
    event_inputs = [(10, "input0", 1), (10, "input0", 0), (20, "input1", 1),
                    (10, "input2", 1), (10, "input7", 1), (30, "input0", 0),
                    (10, "input8", 1), (10, "input2", 1)]
    expected_events = [tag * 10000 + pins[name] * 10 + level for tag, name, level in event_inputs]
    events = [value(log, f"CALLBACK_EVENT_{i}") for i in range(8)]
    ipsr = [value(log, f"CALLBACK_IPSR_{i}") for i in range(8)]
    if events != expected_events or ipsr != [22] * 8 or value(log, "CALLBACK_INTENSET") & 255:
        raise ValueError(f"incorrect callback pin/level/capture/IRQ or still subscribed: {events}, {ipsr}")
    report.update(status="passed", metrics=metrics, stages=stages, events=events, ipsr=ipsr, known_model_warnings=known)
