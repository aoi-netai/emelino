"""nRF52840 controller wiring and observations for shared device scenarios."""
import hashlib
import re
import shutil


class Platform:
    def __init__(self, target):
        self.target = target

    def limitations(self):
        return ["nRF52840 register/IRQ simulation; no electrical timing or physical board validation",
                "UART0 uses the legacy UART controller; SPI2 and TWI0 use non-DMA transfers",
                "bundled SPI model ignores pin selection, clock and mode registers; requested writes are checked",
                "bundled legacy UART model omits ERRORSRC and parts of pin selection; line-error injection is not validated",
                "factory revision words at 0x10000130/0x10000134 are absent; SystemInit reads zero and omits revision-specific errata"]

    def prepare(self, renode, output, report, controller=None, bus=None):
        source = renode.parent / "platforms/cpus/nrf52840.repl"
        report["platform_sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
        platform = "\n".join(line for line in source.read_text().splitlines() if "ApplySVD " not in line) + "\n"
        old_uart = 'uart0: UART.NRF52840_UART @ sysbus 0x40002000\n    easyDMA: true'
        if platform.count(old_uart) != 1:
            raise ValueError("unsupported Renode nRF52840 UART0 registration")
        platform = platform.replace(old_uart, old_uart.replace("easyDMA: true", "easyDMA: false"))
        if controller:
            if bus not in (None, 1):
                raise ValueError("nRF52 target maps I2c1 to TWI0")
            old = 'twi0: I2C.NRF52840_I2C @ sysbus '
            if platform.count(old) != 1:
                raise ValueError("unsupported Renode nRF52840 TWI0 registration")
            platform = platform.replace(old, 'twi0: I2C.EmelinoNRF52840I2C @ sysbus ')
            shutil.copyfile(controller, output / "EmelinoNRF52840I2C.cs")
            report["controller_model"] = str(controller)
            report["controller_sha256"] = hashlib.sha256(controller.read_bytes()).hexdigest()
        elif bus is not None:
            report["controller_model"] = "bundled NRF52840_I2C (baseline)"
        (output / "nrf52840.repl").write_text(platform)

    def machine_header(self, alias):
        return (f'using "{alias}/nrf52840.repl"\n'
                f'nvic:\n    systickFrequency: {self.target["profile"]["clock"]["frequency_hz"]}\n')

    def controller_includes(self, alias):
        return [f'include @{alias}/EmelinoNRF52840I2C.cs']

    def load_binary(self, alias):
        return f'sysbus LoadBinary @{alias}/firmware/firmware.bin 0x0'

    def bmp180_bus(self, settings):
        if settings["i2c"] != 1:
            raise ValueError("nRF52 target currently maps only I2c1 to TWI0")
        return "twi0"

    def bmp180_observe(self, symbols, settings):
        irq = "SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQHandler"
        commands = [f'sysbus.cpu AddHook 0x{symbols[irq] & ~1:x} "self.Log(LogLevel.Info, \'BMP180_EVENT_IRQ\')"']
        mapping = self.target["profile"]["peripherals"]["i2c1"]
        for field in ("scl_pin", "sda_pin"):
            pin = mapping[field]
            commands.append(self.drive(pin, True))
        return commands

    def bmp180_transactions(self, log):
        transactions, current = [], []
        for event in re.findall(r"EMELINO_I2C_(START|STOP)(?:\s|$)", log):
            current.append(event)
            if event == "STOP":
                transactions.append(current)
                current = []
        return transactions, current

    def bmp180_warnings(self, log, settings):
        known = re.compile(r"\[WARNING\] twi0\.bmp180: Unhandled write to offset 0xE0\. Unhandled bits: \[1-2, 4-5, 7\] when writing value 0xB6\.$")
        warnings = [line for line in log.splitlines() if "[WARNING]" in line]
        unknown = [line for line in warnings if not known.search(line) and not self.known_model_warning(line, settings)]
        if unknown:
            raise ValueError("unexpected model warning: " + "\n".join(unknown))
        return warnings

    def known_model_warning(self, line, settings):
        if re.search(r"\[WARNING\] sysbus: \[cpu: 0x[0-9A-Fa-f]+\] \(tag: 'FICR'\) ReadDoubleWord from non existing peripheral at 0x1000013[04], returning 0x00000000\.(?: \(\d+\))?$", line):
            return True
        if "spi" in settings:
            pins = self.target["profile"]["peripherals"]["spi1"]
            expected = {0x508: pins["sck_pin"], 0x50c: pins["mosi_pin"], 0x510: pins["miso_pin"],
                        0x524: 0x10000000, 0x554: 0}
            match = re.search(r"\[WARNING\] spi2: Unhandled write to offset (0x[0-9A-F]+), value (0x[0-9A-F]+)\.$", line)
            if match:
                return expected.get(int(match[1], 16)) == int(match[2], 16)
        if "uart" in settings:
            pins = self.target["profile"]["peripherals"]["uart0"]
            match = re.search(r"\[WARNING\] uart0: Unhandled write to offset (0x50C|0x514)\..* when writing value (0x[0-9A-F]+)\. Tags: .+\.$", line)
            if match:
                expected = pins["tx_pin"] if int(match[1], 16) == 0x50c else pins["rx_pin"]
                return int(match[2], 16) in (expected, 0xffffffff)
            if re.search(r"\[WARNING\] uart0: Unhandled (?:read from offset 0x480\.|write to offset 0x480, value 0x(?:0|F)\.)$", line):
                return True
        return False

    def mt25q_bus(self, settings):
        if settings["spi"] != 1:
            raise ValueError("nRF52 target currently maps only Spi1 to SPI2")
        return "spi2"

    def mt25q_connection(self, settings):
        pin = settings["cs_pin"]
        return f'gpio{pin // 32}:\n    {pin % 32} -> serialFlash@0\n'

    def mt25q_observe(self, settings):
        pin, bus = settings["cs_pin"], self.mt25q_bus(settings)
        sck = self.target["profile"]["peripherals"]["spi1"]["sck_pin"]
        sck_offset = 0x200 + 4 * (sck % 32)
        bit = 1 << (pin % 32)
        commands = [f'sysbus SetHookBeforePeripheralWrite sysbus.{bus} "if offset in [1288, 1292, 1296, 1308, 1316, 1364]: globals()[\'flash_sequence\'] = globals().get(\'flash_sequence\', 0) + 1; self.Log(LogLevel.Info, \'FLASH_TX {{0}} sequence={{2}}\' if offset == 1308 else \'SPI_CONFIG offset={{1}} value={{0}} sequence={{2}}\', value, offset, globals()[\'flash_sequence\'])"']
        # Renode installs one hook per peripheral, so combine CS and SCK when
        # they share a GPIO port instead of replacing either observation.
        for port in sorted({pin // 32, sck // 32}):
            conditions = []
            if port == pin // 32:
                conditions.append(f"(offset in [8, 12] and value & {bit})")
            if port == sck // 32:
                conditions.append(f"offset == {sck_offset}")
            condition = " or ".join(conditions)
            commands.append(f'sysbus SetHookBeforePeripheralWrite sysbus.gpio{port} "if {condition}: globals()[\'gpio_spi_sequence\'] = globals().get(\'gpio_spi_sequence\', 0) + 1; self.Log(LogLevel.Info, \'SPI_SCK_PIN_CNF {{0}} sequence={{1}}\' if offset == {sck_offset} else \'FLASH_CS {{0}} sequence={{1}}\', value if offset == {sck_offset} else (1 if offset == 8 else 0), globals()[\'gpio_spi_sequence\'])"')
        return commands

    def validate_mt25q_configuration(self, log, settings):
        pins = self.target["profile"]["peripherals"]["spi1"]
        expected = {0x508: pins["sck_pin"], 0x50c: pins["mosi_pin"], 0x510: pins["miso_pin"],
                    0x524: 0x10000000, 0x554: 0}
        writes = [(int(offset), int(value)) for offset, value in
                  re.findall(r"SPI_CONFIG offset=(\d+) value=(\d+)", log)]
        if set(offset for offset, _ in writes) != set(expected) or any(expected.get(offset) != value for offset, value in writes):
            raise ValueError("missing or incorrect SPI pin/frequency/mode configuration writes: " + repr(writes))
        sck_writes = [int(value) for value in re.findall(r"SPI_SCK_PIN_CNF (\d+)", log)]
        if not sck_writes or any(value & 3 != 1 for value in sck_writes):
            raise ValueError("SPI SCK must be output with its input buffer connected: " + repr(sck_writes))
        return {"controller_configuration_writes": writes,
                "sck_pin_configuration_writes": {"pin": pins["sck_pin"], "values": sck_writes}}

    def mt25q_cs_read(self, settings):
        base = 0x50000504 if settings["cs_pin"] < 32 else 0x50000804
        return f'sysbus ReadDoubleWord {base:#x}'

    def mt25q_cs_inactive(self, value, settings):
        return bool(value & (1 << (settings["cs_pin"] % 32)))

    def drive(self, pin, level):
        return f'sysbus.gpio{pin // 32} OnGPIO {pin % 32} {str(level).lower()}'
