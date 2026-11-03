"""STM32CubeF4/GCC adapter for the common firmware build entry point."""

import os
from pathlib import Path
import re
import shutil
import struct

from firmware.build.artifact import include_directories
from firmware.build.support import sha256, write_json

HERE = Path(__file__).resolve().parent
COMMON = HERE.parents[1] / "common"
PERIPHERAL_FIELDS = {
    "uart2": {"tx_pin", "rx_pin", "af"},
    "spi1": {"sck_pin", "miso_pin", "mosi_pin", "af"},
    "i2c1": {"scl_pin", "sda_pin", "af"},
}


def compiler_path(value):
    value = value or os.environ.get("ARM_GCC") or "arm-none-eabi-gcc"
    path = Path(value).expanduser()
    if path.is_dir():
        path /= "arm-none-eabi-gcc"
    found = shutil.which(str(path))
    if not found:
        raise ValueError("ARM GCC not found; pass --cc or set ARM_GCC")
    return Path(found).resolve()


def validate(profile, chip, stack_bytes, guard_bytes):
    clock = profile.get("clock")
    if (not isinstance(clock, dict) or type(clock.get("frequency_hz")) is not int
            or clock != {"source": "hsi", "frequency_hz": 16000000}):
        raise ValueError("STM32F4 port currently requires the 16 MHz reset HSI clock")
    if (chip.get("cpu"), chip.get("fpu"), chip.get("float_abi")) != ("cortex-m4", "fpv4-sp-d16", "hard"):
        raise ValueError("unsupported STM32F4 CPU/FPU ABI")
    if not isinstance(chip.get("cmsis_define"), str) or not re.fullmatch(r"STM32F4[A-Za-z0-9]+", chip["cmsis_define"]):
        raise ValueError("invalid STM32F4 CMSIS define")
    for name in ("flash_origin", "flash_bytes", "ram_origin", "ram_bytes"):
        if type(chip.get(name)) is not int or not 0 < chip[name] <= 0xffffffff or chip[name] % 8:
            raise ValueError("invalid STM32F4 memory field: " + name)
    if chip["flash_origin"] + chip["flash_bytes"] > 0x100000000 or chip["ram_origin"] + chip["ram_bytes"] > 0x100000000:
        raise ValueError("STM32F4 memory region exceeds address space")
    if stack_bytes <= 0 or stack_bytes % 8 or guard_bytes < 32 or guard_bytes % 8:
        raise ValueError("stack must be positive and guard at least 32; both must be multiples of 8")
    if stack_bytes + guard_bytes > chip["ram_bytes"]:
        raise ValueError("stack/guard reservation exceeds RAM")
    masks = chip.get("gpio_pin_masks")
    if not isinstance(masks, list) or not 1 <= len(masks) <= 8 or any(type(mask) is not int or not 0 <= mask <= 65535 for mask in masks):
        raise ValueError("invalid STM32F4 package pin masks")
    vectors = chip.get("external_vectors")
    if not isinstance(vectors, list) or not vectors or any(name is not None and (not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name)) for name in vectors):
        raise ValueError("invalid STM32F4 IRQ vector list")
    peripherals = profile.get("peripherals", {})
    if not isinstance(peripherals, dict) or set(peripherals) - PERIPHERAL_FIELDS.keys():
        raise ValueError("STM32F4 port currently maps only uart2, spi1 and i2c1")
    for name, mapping in peripherals.items():
        if not isinstance(mapping, dict) or set(mapping) != PERIPHERAL_FIELDS[name]:
            raise ValueError("invalid peripheral mapping: " + name)
        pins = []
        for key, value in mapping.items():
            if type(value) is not int:
                raise ValueError("peripheral pin/AF must be an integer: " + name)
            if key == "af":
                if not 0 <= value <= 15:
                    raise ValueError("invalid alternate function: " + name)
            else:
                if not 0 <= value < len(masks) * 16 or not masks[value // 16] & (1 << (value % 16)):
                    raise ValueError("pin is not available on this MCU package: " + name)
                pins.append(value)
        if len(set(pins)) != len(pins):
            raise ValueError("peripheral pins must be distinct: " + name)


def configure(output, profile, chip, stack_bytes, guard_bytes, timed=False):
    validate(profile, chip, stack_bytes, guard_bytes)
    masks = chip["gpio_pin_masks"]
    header = ["/* Generated from the board and MCU profiles. */",
              "#ifndef EMELINO_STM32F4_BOARD_CONFIG_H",
              "#define EMELINO_STM32F4_BOARD_CONFIG_H",
              f"#define EMELA_GPIO_PORT_COUNT {len(masks)}",
              "#define EMELA_GPIO_PIN_MASKS {" + ", ".join(hex(mask) for mask in masks) + "}",
              f"#define EMELA_EXTERNAL_IRQ_COUNT {len(chip['external_vectors'])}",
              f"#define EMELA_BOARD_CLOCK_HZ {profile['clock']['frequency_hz']}"]
    header += [f"#define EMELA_{name.upper()}_{key.upper()} {value}"
               for name, pins in profile.get("peripherals", {}).items()
               for key, value in pins.items()]
    if timed:
        header.append("#define EMELA_PERIPHERAL_TIME 1")
    (output / "board_config.h").write_text("\n".join(header + ["#endif", ""]))
    vectors = ["/* IRQ slots from the MCU's CMSIS vector table. */"]
    vectors += ["    vector " + name if name else "    .word 0"
                for name in chip["external_vectors"]]
    (output / "board_vectors.inc").write_text("\n".join(vectors) + "\n")
    linker = (HERE / "firmware.ld.in").read_text()
    values = {"FLASH_ORIGIN": chip["flash_origin"], "FLASH_BYTES": chip["flash_bytes"],
              "RAM_ORIGIN": chip["ram_origin"], "RAM_BYTES": chip["ram_bytes"],
              "STACK_BYTES": stack_bytes, "GUARD_BYTES": guard_bytes}
    for key, value in values.items():
        linker = linker.replace("@" + key + "@", hex(value))
    if re.search(r"@[A-Z_]+@", linker):
        raise ValueError("unresolved linker template placeholder")
    (output / "firmware.ld").write_text(linker)


def build(args, profile, chip, board_dir, inputs, metadata, run):
    generated, output = args.generated_dir.resolve(), args.output_dir.resolve()
    unsupported = set(metadata["services"]) - {
        "service.gpio", "service.uart", "service.spi", "service.i2c", "runtime.defect", "runtime.lifecycle"}
    if unsupported:
        raise ValueError("STM32F4 BSP does not implement: " + ", ".join(sorted(unsupported)))
    drivers = {name.removeprefix("service.") for name in metadata["services"] if name.startswith("service.")}
    if "spi" in drivers:
        drivers.add("gpio")
    timed = bool(drivers & {"uart", "spi", "i2c"})
    configure(output, profile, chip, args.stack_bytes, args.guard_bytes, timed)
    for driver, mapping in {"uart": "uart2", "spi": "spi1", "i2c": "i2c1"}.items():
        if driver in drivers and mapping not in profile.get("peripherals", {}):
            raise ValueError("board has no mapping for required service: " + driver)
    cc = compiler_path(args.cc)
    sdk_arg = args.sdk_dir or os.environ.get("STM32CUBE_F4")
    if not sdk_arg:
        raise ValueError("STM32CubeF4 SDK required; pass --sdk-dir or set STM32CUBE_F4")
    sdk = Path(sdk_arg).expanduser().resolve()
    hal = sdk / "Drivers/STM32F4xx_HAL_Driver"
    cmsis = sdk / "Drivers/CMSIS"
    prefix = str(cc).removesuffix("gcc")
    sdk_sources = [hal / "Src/stm32f4xx_hal_gpio.c"]
    sdk_sources += [hal / f"Src/stm32f4xx_hal_{name}.c" for name in sorted(drivers - {"gpio"})]
    if timed:
        sdk_sources.append(hal / "Src/stm32f4xx_hal_dma.c")
    sources = [HERE / "startup.S", HERE / "platform.c", HERE / "pins.c",
               *([board_dir / "init.c"] if (board_dir / "init.c").is_file() else []),
               *([HERE / "time.c"] if timed else []),
               *([COMMON / "spi.c"] if "spi" in drivers else []),
               *[HERE / (name + ".c") for name in sorted(drivers)],
               *sdk_sources, *inputs["source"], *[path.resolve() for path in args.source]]
    for source in sources:
        if not source.is_file():
            raise ValueError(f"missing build input: {source}")
    arch = ["-mcpu=" + chip["cpu"], "-mthumb", "-mfpu=" + chip["fpu"],
            "-mfloat-abi=" + chip["float_abi"]]
    flags = arch + ["-" + args.optimization, "-g3", "-std=c11", "-ffunction-sections",
                    "-fdata-sections", "-Wall", "-Wextra", "-Werror", "-fstack-usage",
                    "-D" + chip["cmsis_define"], "-DUSE_HAL_DRIVER"]
    includes = {output, HERE, COMMON, board_dir, generated, hal / "Inc", cmsis / "Include",
                cmsis / "Device/ST/STM32F4xx/Include", *include_directories(inputs)}
    flags += ["-I" + str(path) for path in sorted(includes)]
    objects_dir = output / "objects"
    objects_dir.mkdir(exist_ok=True)
    version = run([cc, "--version"]).splitlines()[0]
    objects = []
    for index, source in enumerate(sources):
        obj = objects_dir / f"{index}-{source.name}.o"
        run([cc, *flags, "-c", source, "-o", obj])
        objects.append(obj)
    elf = output / "firmware.elf"
    run([cc, *arch, "-nostartfiles", "--specs=nano.specs", "--specs=nosys.specs",
         "-T" + str(output / "firmware.ld"), "-Wl,--gc-sections",
         "-Wl,-Map," + str(output / "firmware.map"), *objects, *args.link_arg, "-lm", "-o", elf])
    symbols = {}
    for line in run([prefix + "nm", "-S", "--defined-only", elf]).splitlines():
        fields = line.split()
        if len(fields) in (3, 4):
            symbols[fields[-1]] = int(fields[0], 16)
    forbidden = re.compile(r"(?:malloc|calloc|realloc|free|sbrk|_sbrk|_sbrk_r|_malloc_r|_calloc_r|_realloc_r|_free_r)")
    if any(forbidden.fullmatch(name) for name in symbols):
        raise ValueError("firmware unexpectedly links a system allocator")
    run([prefix + "objcopy", "-O", "binary", elf, output / "firmware.bin"])
    size_text = run([prefix + "size", elf])
    (output / "size.txt").write_text(size_text)
    image = (output / "firmware.bin").read_bytes()
    if not 8 <= len(image) <= chip["flash_bytes"]:
        raise ValueError("firmware exceeds Flash or has no reset vectors")
    initial_sp, reset = struct.unpack_from("<II", image)
    if (initial_sp != chip["ram_origin"] + chip["ram_bytes"] or not reset & 1
            or not chip["flash_origin"] <= (reset & ~1) < chip["flash_origin"] + len(image)):
        raise ValueError("invalid initial SP/reset vector")
    if symbols["__emela_vector_table_end__"] - symbols["g_pfnVectors"] != 4 * (16 + len(chip["external_vectors"])):
        raise ValueError("invalid MCU vector table length")
    resources = {"flash_bytes": len(image), "flash_capacity": chip["flash_bytes"],
                 "static_ram_bytes": symbols["__emela_static_ram_end__"] - chip["ram_origin"],
                 "stack_reserved_bytes": args.stack_bytes,
                 "stack_guard_bytes": args.guard_bytes, "ram_capacity": chip["ram_bytes"],
                 "system_allocator": False}
    write_json(output / "resources.json", resources)
    write_json(output / "symbols.json", symbols)
    print(size_text.strip())
    print(output / "firmware.elf")
    return {"compiler": version,
            "sdk": {"directory": str(sdk), "gpio_driver_sha256": sha256(hal / "Src/stm32f4xx_hal_gpio.c")},
            "sdk_sources": {path.name: sha256(path) for path in sdk_sources},
            "resources": resources,
            "artifacts": ["firmware.elf", "firmware.bin", "firmware.map", "resources.json", "symbols.json"]}
