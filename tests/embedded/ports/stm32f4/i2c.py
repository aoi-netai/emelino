"""Scenario steps and assertions; execution is provided by the common runner."""
import re
import shutil
from support.common import MODELS, build_firmware, renode_workspace, run_renode, value

def test(args, report):
    output, renode, manifest, symbols = build_firmware(args, report, "i2c")
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"]})
    report["model_limitations"] = ["generic STM32F4, not STM32F446 silicon",
        "I2C peer supplies a register window because Renode 1.16.1 requests Read(1) once",
        "one-byte receive is tested as a known model Timeout: SR2 alone clears ADDR and BTF is unconditional",
        "NACK mapping/retries use a test-only HAL completion wrapper; the old model clears AF on SR1 read",
        "I2C POS bit, open-drain drive and external pull resistors are not electrically modeled",
        "recovery tests controller reset and pin sequence, not a physically stuck bus"]
    args.platform.prepare(renode, output, report)
    shutil.copyfile(MODELS / "stm32f4-i2c-legacy/I2cPeer.cs", output / "I2cPeer.cs")
    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(args.platform.machine_header(alias) +
            'peer: I2C.EmelinoI2cPeer @ i2c1 0x50\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
            f'include @{alias}/I2cPeer.cs', 'mach create',
            f'machine LoadPlatformDescription @{alias}/machine.repl',
            'sysbus SetHookBeforePeripheralWrite sysbus.i2c1 "if offset == 0: self.Log(LogLevel.Info, \'EMELINO_I2C_CR1 value={0}\', value)"',
            f'sysbus.cpu AddHook 0x{symbols["I2C1_EV_IRQHandler"] & ~1:x} "self.Log(LogLevel.Info, \'EMELINO_I2C_EV_IRQ\')"',
            args.platform.load_binary(alias),
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
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
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
