"""STM32F4 model preparation and register observations, separate from device cases."""
import hashlib
import re
import shutil


class Platform:
    def __init__(self, target):
        self.target = target

    def limitations(self):
        return ["generic STM32F4 register/IRQ model, not STM32F446 silicon or electrical timing"]

    def prepare(self, renode, output, report, controller=None, bus=None):
        original = renode.parent / "platforms/cpus/stm32f4.repl"
        report["platform_sha256"] = hashlib.sha256(original.read_bytes()).hexdigest()
        platform = "\n".join(line for line in original.read_text().splitlines() if "ApplySVD " not in line) + "\n"
        flash = "flash: Memory.MappedMemory @ sysbus 0x08000000"
        if platform.count(flash) != 1 or self.target["chip"]["flash_origin"] != 0x08000000:
            raise ValueError("unsupported Renode platform Flash registration")
        platform = platform.replace(flash, "flash: Memory.MappedMemory @ { sysbus 0x08000000; sysbus 0x00000000 }")
        if controller:
            source = f"i2c{bus}: I2C.STM32F4_I2C @ sysbus "
            if platform.count(source) != 1:
                raise ValueError("unsupported Renode I2C registration")
            platform = platform.replace(source, f"i2c{bus}: I2C.EmelinoSTM32F4I2C @ sysbus ")
            shutil.copyfile(controller, output / "EmelinoSTM32F4I2C.cs")
            report["controller_model"] = str(controller)
            report["controller_sha256"] = hashlib.sha256(controller.read_bytes()).hexdigest()
        elif bus is not None:
            report["controller_model"] = "bundled STM32F4_I2C (baseline)"
        (output / "stm32f4.repl").write_text(platform)

    def machine_header(self, alias):
        chip, board = self.target["chip"], self.target["profile"]
        return (f'using "{alias}/stm32f4.repl"\n'
                f'nvic:\n    systickFrequency: {board["clock"]["frequency_hz"]}\n'
                f'flash:\n    size: {chip["flash_bytes"]:#x}\nsram:\n    size: {chip["ram_bytes"]:#x}\n')

    def controller_includes(self, alias):
        return [f'include @{alias}/EmelinoSTM32F4I2C.cs']

    def load_binary(self, alias):
        return f'sysbus LoadBinary @{alias}/firmware/firmware.bin {self.target["chip"]["flash_origin"]:#x}'

    def bmp180_bus(self, settings):
        return f'i2c{settings["i2c"]}'

    def bmp180_observe(self, symbols, settings):
        bus = self.bmp180_bus(settings)
        pins = self.target["profile"]["peripherals"][bus]
        commands = [
            f'sysbus SetHookBeforePeripheralWrite sysbus.{bus} "if offset in [0, 16]: self.Log(LogLevel.Info, \'BMP180_BUS offset={{0}} value={{1}}\', offset, value)"',
            f'sysbus.cpu AddHook 0x{symbols[bus.upper() + "_EV_IRQHandler"] & ~1:x} "self.Log(LogLevel.Info, \'BMP180_EVENT_IRQ\')"']
        for field in ("scl_pin", "sda_pin"):
            pin = pins[field]
            commands.append(f'sysbus.gpioPort{chr(65 + pin // 16)} OnGPIO {pin % 16} true')
        return commands

    def bmp180_transactions(self, log):
        transactions, current = [], []
        for item in re.findall(r"BMP180_BUS offset=0 value=(\d+)", log):
            bits = int(item)
            if bits & 0x100:
                current.append("START")
            if bits & 0x200:
                current.append("STOP")
                transactions.append(current)
                current = []
        return transactions, current

    def bmp180_warnings(self, log, settings):
        bus = self.bmp180_bus(settings)
        patterns = []
        pins = self.target["profile"]["peripherals"][bus]
        for field in ("scl_pin", "sda_pin"):
            pin = pins[field]
            patterns.append(re.escape(f'gpioPort{chr(65 + pin // 16)}: Unhandled write to offset 0x4. Unhandled bits: [{pin % 16}] when writing value 0x{1 << (pin % 16):X}. Tags: OT{pin % 16} (0x1).'))
        patterns += [re.escape(f'{bus}.bmp180: Unhandled write to offset 0xE0. Unhandled bits: [1-2, 4-5, 7] when writing value 0xB6.'),
                     re.escape(f'{bus}: Unhandled write to offset 0x14. Unhandled bits: [3-5, 8-9, 11-31] when writing value 0xFFFFFBFF.')]
        known = re.compile(r"\[WARNING\] (?:" + "|".join(patterns) + ")$")
        warnings = [line for line in log.splitlines() if "[WARNING]" in line]
        unknown = [line for line in warnings if not known.search(line)]
        if unknown:
            raise ValueError("unexpected model warning: " + "\n".join(unknown))
        return warnings

    def mt25q_bus(self, settings):
        return f'spi{settings["spi"]}'

    def mt25q_connection(self, settings):
        pin = settings["cs_pin"]
        return f'gpioPort{chr(65 + pin // 16)}:\n    {pin % 16} -> serialFlash@0\n'

    def mt25q_observe(self, settings):
        pin, bus = settings["cs_pin"], self.mt25q_bus(settings)
        mask = (1 << (pin % 16)) | (1 << (pin % 16 + 16))
        return [
            f'sysbus SetHookBeforePeripheralWrite sysbus.{bus} "if offset == 12: globals()[\'flash_sequence\'] = globals().get(\'flash_sequence\', 0) + 1; self.Log(LogLevel.Info, \'FLASH_TX {{0}} sequence={{1}}\', value, globals()[\'flash_sequence\'])"',
            f'sysbus SetHookBeforePeripheralWrite sysbus.gpioPort{chr(65 + pin // 16)} "if offset == 24 and value & {mask}: self.Log(LogLevel.Info, \'FLASH_CS {{0}}\', 0 if value & {1 << (pin % 16 + 16)} else 1)"']

    def mt25q_cs_read(self, settings):
        return f'sysbus ReadDoubleWord {0x40020014 + (settings["cs_pin"] // 16) * 0x400:#x}'

    def mt25q_cs_inactive(self, odr, settings):
        return bool(odr & (1 << (settings["cs_pin"] % 16)))
