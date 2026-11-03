"""Scenario steps and assertions; execution is provided by the common runner."""
import re
import shutil
from support.common import MODELS, build_firmware, renode_workspace, run_renode, value

def test(args, report):
    output, renode, manifest, symbols = build_firmware(args, report, "spi")
    report.update({"board": manifest["board"], "resources": manifest["resources"],
                   "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"]})
    report["model"] = "Renode generic STM32F4, SPI1 deterministic byte peer, F446RE memory sizes"
    report["model_limitations"] = ["not an STM32F446 silicon model",
                                   "SPI peer exchanges bytes only; 16-bit DR writes are observed separately",
                                   "SCK frequency, CPOL/CPHA and bit order are register configuration checks",
                                   "CS setup/hold uses virtual time; electrical timing is not validated",
                                   "primary/cleanup combination uses test-only linker wrappers",
                                   "HAL polling timeout is tested by withholding RXNE with a bus read hook"]
    args.platform.prepare(renode, output, report)
    shutil.copyfile(MODELS / "SpiPeer.cs", output / "SpiPeer.cs")
    names = ["emela_board_completed", "emela_board_failures", "emela_board_boot_initialized",
             "spi_test_stage", "spi_test_checks", "spi_test_failures", "spi_test_last_failure",
             "spi_test_injected_timeouts", "spi_test_injected_cleanup_errors"]

    with renode_workspace(output) as (alias, temporary):
        (output / "machine.repl").write_text(
            args.platform.machine_header(alias) +
            'peer: SPI.EmelinoSpiPeer @ spi1\n'
            'gpioPortA:\n    4 -> peer@0\n    3 -> peer@1\n    2 -> peer@2\n    1 -> peer@3\n')
        script = [f'logFile @{alias}/model.log', 'logLevel 1 file', 'logLevel 3 console',
                  f'include @{alias}/SpiPeer.cs', 'mach create',
                  f'machine LoadPlatformDescription @{alias}/machine.repl',
                  'sysbus SetHookBeforePeripheralWrite sysbus.spi1 "if offset in [0, 12]: self.Log(LogLevel.Info, \'EMELINO_SPI_{0} value={1}\', \'DR\' if offset == 12 else \'CR1\', value)"',
                  f'sysbus SetHookAfterPeripheralRead sysbus.spi1 "if offset == 8 and sysbus.ReadDoubleWord(0x{symbols["spi_test_injection"]:x}) == 3: value = value & ~1"',
                  args.platform.load_binary(alias),
                  'sysbus.cpu PerformanceInMips 100', 'sysbus.cpu Reset',
                  'emulation RunFor "0:00:00.050"',
                  f'sysbus WriteDoubleWord 0x{symbols["spi_test_command"]:x} 1',
                  'emulation RunFor "0:00:00.005"',
                  f'sysbus WriteDoubleWord 0x{symbols["spi_test_command"]:x} 2',
                  'emulation RunFor "0:00:00.005"']
        for name in names:
            script += [f'echo "SPI_FINAL_{name}"', f'sysbus ReadDoubleWord 0x{symbols[name]:x}']
        for label, address in {"MODER": 0x40020000, "AFRL": 0x40020020,
                               "ODR": 0x40020014, "RCC_APB2ENR": 0x40023844}.items():
            script += [f'echo "SPI_REGISTER_{label}"', f'sysbus ReadDoubleWord 0x{address:x}']
        script.append('quit')
        status, log = run_renode(renode, output, alias, temporary, script, report, args.timeout)
    model_log = (output / "model.log").read_text(encoding="utf-8-sig")
    # This model tags DFF but only exchanges bytes. Every other warning is a failure.
    known = re.compile(r"^.*\[WARNING\] spi1: Unhandled write to offset 0x0\. Unhandled bits: \[11\] when writing value 0x[0-9A-Fa-f]+\. Tags: DFF \(0x1\)\.\s*$", re.M)
    initial_disable = re.compile(r"^.*\[WARNING\] spi1: Slave mode is not supported\s*$", re.M)
    controls = [int(number) for number in re.findall(r"EMELINO_SPI_CR1 value=(\d+)", model_log)]
    if len(initial_disable.findall(model_log)) != 1 or not controls or controls[0] != 0 or controls.count(0) != 1:
        raise ValueError("unexpected SPI master/slave configuration; HAL's initial disabled CR1 write must be the only zero")
    report["known_model_warnings"] = [line.strip() for line in known.findall(model_log) + initial_disable.findall(model_log)]
    combined = initial_disable.sub("", known.sub("", log + model_log))
    if status or re.search(r"\[(?:ERROR|WARNING)\]|There was an error executing command|Unhandled Exception", combined):
        raise ValueError(f"Renode failed or emitted a warning; see {output / 'renode.log'} and model.log")
    metrics = {name: value(log, "SPI_FINAL_" + name) for name in names}
    expected = {"emela_board_completed": 1, "emela_board_failures": 0, "emela_board_boot_initialized": 1,
                "spi_test_stage": 2, "spi_test_checks": 32, "spi_test_failures": 0,
                "spi_test_last_failure": 0, "spi_test_injected_timeouts": 1,
                "spi_test_injected_cleanup_errors": 2}
    if metrics != expected:
        raise ValueError(f"firmware checks failed: {metrics!r}")
    transmitted = [int(number) for number in re.findall(r"EMELINO_SPI tx=(\d+)", model_log)]
    expected_tx = [0, 1, 127, 255, 16, 32, 255, 255, 255,
                   1, 2, 255, 255, 3, 4, 5, 6, 7, 255, 52, 255, 9, 10]
    if transmitted != expected_tx:
        raise ValueError(f"SPI peer byte mismatch: {transmitted!r}")
    word_writes = [int(number) for number in re.findall(r"EMELINO_SPI_DR value=(\d+)", model_log)]
    if word_writes != expected_tx[:-4] + [4660, 65535, 9, 10]:
        raise ValueError(f"SPI DR word mismatch: {word_writes!r}")
    cs = [[int(level) for level in re.findall(rf"EMELINO_SPI_CS pin={pin} level=(\d+)", model_log)]
          for pin in range(4)]
    if cs != [[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1], [0, 1, 0], [0, 1], [0, 1]]:
        raise ValueError(f"chip-select transitions differ: {cs!r}")
    if value(log, "SPI_REGISTER_MODER") & 0xfc00 != 0xa800:
        raise ValueError("SPI bus pins are not in alternate-function mode")
    if value(log, "SPI_REGISTER_AFRL") & 0xfff00000 != 0x55500000:
        raise ValueError("SPI bus pins do not select AF5")
    if value(log, "SPI_REGISTER_ODR") & 0x18 != 0x10:
        raise ValueError("chip selects are not both inactive")
    if not value(log, "SPI_REGISTER_RCC_APB2ENR") & 0x1000:
        raise ValueError("SPI1 clock is disabled")
    report.update({"status": "passed", "metrics": metrics, "transmitted_bytes": transmitted,
                   "data_register_writes": word_writes, "chip_select_transitions": cs})
