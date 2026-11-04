"""Nordic nrfx MDK/CMSIS adapter for nRF52840 firmware without an RTOS."""

import os
from pathlib import Path
import re

from firmware.build import arm_gcc
from firmware.build.artifact import include_directories
from firmware.build.support import sha256

HERE = Path(__file__).resolve().parent
COMMON = HERE.parents[1] / "common"
ROOT = HERE.parents[2]
PERIPHERAL_FIELDS = {
    "uart0": {"tx_pin", "rx_pin"},
    "spi1": {"sck_pin", "miso_pin", "mosi_pin"},
    "i2c1": {"scl_pin", "sda_pin"},
}


def validate(profile, chip, stack_bytes, guard_bytes):
    clock = profile.get("clock")
    if (not isinstance(clock, dict) or type(clock.get("frequency_hz")) is not int
            or clock != {"source": "hfint", "frequency_hz": 64000000}):
        raise ValueError("nRF52 port requires the reset 64 MHz HFINT clock")
    if (chip.get("cpu"), chip.get("fpu"), chip.get("float_abi"), chip.get("cmsis_define")) != (
            "cortex-m4", "fpv4-sp-d16", "hard", "NRF52840_XXAA"):
        raise ValueError("unsupported nRF52 CPU/device ABI")
    for name in ("flash_origin", "flash_bytes", "ram_origin", "ram_bytes"):
        if type(chip.get(name)) is not int or not 0 <= chip[name] <= 0xffffffff or chip[name] % 8:
            raise ValueError("invalid nRF52 memory field: " + name)
    if (chip["flash_origin"] != 0 or not 0 < chip["flash_bytes"] <= 0x100000
            or chip["ram_origin"] != 0x20000000 or not 0 < chip["ram_bytes"] <= 0x40000):
        raise ValueError("nRF52840 requires raw Flash at zero and SRAM within its hardware range")
    if type(stack_bytes) is not int or type(guard_bytes) is not int or stack_bytes <= 0 or stack_bytes % 8 or guard_bytes < 32 or guard_bytes % 8:
        raise ValueError("stack must be positive and guard at least 32; both must be multiples of 8")
    if stack_bytes + guard_bytes > chip["ram_bytes"]:
        raise ValueError("stack/guard reservation exceeds RAM")
    package = chip.get("gpio_pin_masks")
    available = profile.get("gpio_pin_masks", package)
    analog = chip.get("analog_pin_masks")
    for name, masks in (("package", package), ("board", available), ("analog", analog)):
        if (not isinstance(masks, list) or len(masks) != 2
                or any(type(mask) is not int or not 0 <= mask <= limit
                       for mask, limit in zip(masks, (0xffffffff, 0xffff)))):
            raise ValueError("invalid nRF52840 " + name + " pin masks")
    if any(board & ~physical or adc & ~physical for board, adc, physical in zip(available, analog, package)):
        raise ValueError("pin masks exceed MCU package")
    vectors = chip.get("external_vectors")
    if (not isinstance(vectors, list) or len(vectors) != 48
            or any(name is not None and (not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name)) for name in vectors)):
        raise ValueError("invalid nRF52840 IRQ vector list")
    peripherals = profile.get("peripherals", {})
    if not isinstance(peripherals, dict) or set(peripherals) - PERIPHERAL_FIELDS.keys():
        raise ValueError("nRF52 port currently maps only uart0, spi1 and i2c1")
    for name, mapping in peripherals.items():
        if not isinstance(mapping, dict) or set(mapping) != PERIPHERAL_FIELDS[name]:
            raise ValueError("invalid peripheral mapping: " + name)
        pins = list(mapping.values())
        for pin in pins:
            if (type(pin) is not int or not 0 <= pin < 48
                    or not available[pin // 32] & (1 << (pin % 32))):
                raise ValueError("pin is not available on this board: " + name)
        if len(set(pins)) != len(pins):
            raise ValueError("peripheral pins must be distinct: " + name)


def configure(output, profile, chip, stack_bytes, guard_bytes, timed=False):
    validate(profile, chip, stack_bytes, guard_bytes)
    header = ["/* Generated from the board and MCU profiles. */",
              "#ifndef EMELINO_NRF52_BOARD_CONFIG_H", "#define EMELINO_NRF52_BOARD_CONFIG_H",
              "#define EMELA_GPIO_PORT_COUNT 2",
              "#define EMELA_GPIO_PIN_MASKS {" + ", ".join(hex(mask) + "U" for mask in profile.get("gpio_pin_masks", chip["gpio_pin_masks"])) + "}",
              "#define EMELA_ANALOG_PIN_MASKS {" + ", ".join(hex(mask) + "U" for mask in chip["analog_pin_masks"]) + "}",
              "#define EMELA_EXTERNAL_IRQ_COUNT 48",
              f"#define EMELA_BOARD_CLOCK_HZ {profile['clock']['frequency_hz']}"]
    header += [f"#define EMELA_{name.upper()}_{key.upper()} {value}"
               for name, pins in profile.get("peripherals", {}).items()
               for key, value in pins.items()]
    if timed:
        header.append("#define EMELA_PERIPHERAL_TIME 1")
    (output / "board_config.h").write_text("\n".join(header + ["#endif", ""]))
    arm_gcc.configure(output, chip, stack_bytes, guard_bytes)


def sdk_paths(args):
    sdk = Path(args.sdk_dir or os.environ.get("NRFX_ROOT") or ROOT / ".local/embedded-sdk/nrfx-3.2.0").expanduser().resolve()
    cmsis = Path(os.environ.get("CMSIS_ROOT") or sdk.parent / "cmsis-5.9.0").expanduser().resolve()
    include = next((path for path in (cmsis / "CMSIS/Core/Include", cmsis / "Core/Include", cmsis)
                    if (path / "core_cm4.h").is_file()), None)
    if not (sdk / "mdk/nrf52840.h").is_file():
        raise ValueError("Nordic nrfx MDK required; pass --sdk-dir or set NRFX_ROOT")
    if include is None:
        raise ValueError("CMSIS Cortex-M headers required; set CMSIS_ROOT")
    return sdk, include


def build(args, profile, chip, board_dir, inputs, metadata, run):
    generated, output = args.generated_dir.resolve(), args.output_dir.resolve()
    unsupported = set(metadata["services"]) - {
        "service.gpio", "service.uart", "service.spi", "service.i2c", "runtime.defect", "runtime.lifecycle"}
    if unsupported:
        raise ValueError("nRF52 BSP does not implement: " + ", ".join(sorted(unsupported)))
    drivers = {name.removeprefix("service.") for name in metadata["services"] if name.startswith("service.")}
    if "spi" in drivers:
        drivers.add("gpio")
    timed = bool(drivers & {"uart", "spi", "i2c"})
    configure(output, profile, chip, args.stack_bytes, args.guard_bytes, timed)
    for driver, mapping in (("uart", "uart0"), ("spi", "spi1"), ("i2c", "i2c1")):
        if driver in drivers and mapping not in profile.get("peripherals", {}):
            raise ValueError("board has no mapping for required service: " + driver)
    sdk, cmsis = sdk_paths(args)
    system = sdk / "mdk/system_nrf52840.c"
    sources = [arm_gcc.CORTEX_M4F / "startup.S", arm_gcc.CORTEX_M4F / "platform.c", HERE / "pins.c", system,
               *([board_dir / "init.c"] if (board_dir / "init.c").is_file() else []),
               *([HERE / "time.c"] if timed else []),
               *([COMMON / "spi.c"] if "spi" in drivers else []),
               *[HERE / (name + ".c") for name in sorted(drivers)],
               *inputs["source"], *[path.resolve() for path in args.source]]
    includes = {output, HERE, COMMON, board_dir, generated, sdk / "mdk", cmsis, *include_directories(inputs)}
    result = arm_gcc.build(args, chip, sources, includes, [chip["cmsis_define"], "EMELA_CALL_SYSTEM_INIT"], run)
    result.update({"sdk": {"name": "Nordic nrfx MDK + Arm CMSIS", "directory": str(sdk),
                           "cmsis_include": str(cmsis), "system_init": True},
                   "sdk_sources": {str(path.relative_to(sdk)): sha256(path)
                       for path in (system, sdk / "mdk/system_nrf52.c", sdk / "mdk/nrf52840.h",
                                    sdk / "mdk/nrf52840_bitfields.h", sdk / "mdk/nrf52_erratas.h")}})
    return result
