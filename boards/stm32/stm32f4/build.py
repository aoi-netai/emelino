#!/usr/bin/env python3
"""Link a generated Emelino artifact with the STM32F4 peripheral BSP."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "common"))
from artifact import include_directories, load_manifest


def compiler_path(value):
    value = value or os.environ.get("ARM_GCC") or "arm-none-eabi-gcc"
    path = Path(value).expanduser()
    if path.is_dir():
        path /= "arm-none-eabi-gcc"
    found = shutil.which(str(path))
    if not found:
        raise ValueError("ARM GCC not found; pass --cc or set ARM_GCC")
    return Path(found).resolve()


def configure(output, board, timed=False):
    if not re.fullmatch(r"[a-z0-9-]+", board):
        raise ValueError("invalid board name")
    profile = json.loads((HERE / "boards" / (board + ".json")).read_text())
    chip = json.loads((HERE / "chips" / (profile["chip"] + ".json")).read_text())
    if profile["board"] != board or profile["chip"] != chip["id"]:
        raise ValueError("board and chip profiles disagree")
    masks = chip["gpio_pin_masks"]
    header = ["/* Generated from the board and MCU profiles. */",
              "#ifndef EMELINO_STM32F4_BOARD_CONFIG_H",
              "#define EMELINO_STM32F4_BOARD_CONFIG_H",
              f"#define EMELA_GPIO_PORT_COUNT {len(masks)}",
              "#define EMELA_GPIO_PIN_MASKS {" + ", ".join(hex(mask) for mask in masks) + "}",
              f"#define EMELA_EXTERNAL_IRQ_COUNT {len(chip['external_vectors'])}",
              "#endif", ""]
    header[-2:-2] = [f"#define EMELA_{name.upper()}_{key.upper()} {value}"
                      for name, pins in profile.get("peripherals", {}).items()
                      for key, value in pins.items()]
    if timed:
        header.insert(-2, "#define EMELA_PERIPHERAL_TIME 1")
    (output / "board_config.h").write_text("\n".join(header))
    vectors = ["/* IRQ slots from the MCU's CMSIS vector table. */"]
    vectors += ["    vector " + name if name else "    .word 0"
                for name in chip["external_vectors"]]
    (output / "board_vectors.inc").write_text("\n".join(vectors) + "\n")
    linker = (HERE / "firmware.ld.in").read_text()
    values = {"FLASH_ORIGIN": profile["flash_origin"], "FLASH_BYTES": chip["flash_bytes"],
              "RAM_ORIGIN": profile["ram_origin"], "RAM_BYTES": chip["ram_bytes"],
              "STACK_BYTES": profile["stack_bytes"], "GUARD_BYTES": profile["guard_bytes"]}
    for key, value in values.items():
        linker = linker.replace("@" + key + "@", hex(value))
    if re.search(r"@[A-Z_]+@", linker):
        raise ValueError("unresolved linker template placeholder")
    (output / "firmware.ld").write_text(linker)
    return profile, chip


def build(args):
    generated, output = args.generated_dir.resolve(), args.output_dir.resolve()
    if generated == output:
        raise ValueError("generated and firmware output directories must differ")
    output.mkdir(parents=True, exist_ok=True)
    # A failed rebuild must never leave an earlier success marker.
    (output / "firmware.json").unlink(missing_ok=True)
    inputs = load_manifest(generated, require_aliases=True)
    metadata = json.loads((generated / "emela_artifact.json").read_text())["metadata"]
    unsupported = set(metadata["services"]) - {
        "service.gpio", "service.uart", "service.spi", "service.i2c", "runtime.defect", "runtime.lifecycle"}
    if unsupported:
        raise ValueError("STM32F4 BSP does not implement: " + ", ".join(sorted(unsupported)))
    if not metadata["entry"]:
        raise ValueError("firmware requires an entry in the Emelino configuration")
    drivers = set(name.removeprefix("service.") for name in metadata["services"] if name.startswith("service."))
    if "spi" in drivers: drivers.add("gpio")
    timed = bool(drivers & {"uart", "spi", "i2c"})
    profile, chip = configure(output, args.board, timed)
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
    if timed: sdk_sources.append(hal / "Src/stm32f4xx_hal_dma.c")
    sources = [HERE / "startup.S", HERE / "platform.c", HERE / "pins.c",
               *([HERE / "time.c"] if timed else []),
               *[HERE / (name + ".c") for name in sorted(drivers)],
               *sdk_sources, *inputs["source"], *[path.resolve() for path in args.source]]
    for source in sources:
        if not source.is_file():
            raise ValueError(f"missing build input: {source}")
    arch = ["-mcpu=" + profile["cpu"], "-mthumb", "-mfpu=" + profile["fpu"],
            "-mfloat-abi=" + profile["float_abi"]]
    flags = arch + ["-" + args.optimization, "-g3", "-std=c11", "-ffunction-sections",
                    "-fdata-sections", "-Wall", "-Wextra", "-Werror", "-fstack-usage",
                    "-DSTM32F446xx", "-DUSE_HAL_DRIVER"]
    includes = {output, HERE, generated, hal / "Inc", cmsis / "Include",
                cmsis / "Device/ST/STM32F4xx/Include", *include_directories(inputs)}
    flags += ["-I" + str(path) for path in sorted(includes)]
    objects_dir = output / "objects"
    objects_dir.mkdir(exist_ok=True)
    with (output / "build.log").open("w") as log:
        def run(command):
            command = list(map(str, command))
            log.write("$ " + " ".join(command) + "\n")
            log.flush()
            result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    text=True, timeout=120)
            log.write(result.stdout)
            log.flush()
            if result.returncode:
                raise ValueError(f"command failed ({result.returncode}); see {output / 'build.log'}\n{result.stdout}")
            return result.stdout
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
    if (initial_sp != profile["ram_origin"] + chip["ram_bytes"] or not reset & 1
            or not profile["flash_origin"] <= (reset & ~1) < profile["flash_origin"] + len(image)):
        raise ValueError("invalid initial SP/reset vector")
    if symbols["__emela_vector_table_end__"] - symbols["g_pfnVectors"] != 4 * (16 + len(chip["external_vectors"])):
        raise ValueError("invalid MCU vector table length")
    resources = {"flash_bytes": len(image), "flash_capacity": chip["flash_bytes"],
                 "static_ram_bytes": symbols["__emela_static_ram_end__"] - profile["ram_origin"],
                 "stack_reserved_bytes": profile["stack_bytes"],
                 "stack_guard_bytes": profile["guard_bytes"], "ram_capacity": chip["ram_bytes"],
                 "system_allocator": False}
    (output / "resources.json").write_text(json.dumps(resources, indent=2) + "\n")
    (output / "symbols.json").write_text(json.dumps(symbols, indent=2, sort_keys=True) + "\n")
    artifacts = ["firmware.elf", "firmware.bin", "firmware.map", "resources.json", "symbols.json"]
    manifest = {"schema_version": 1, "board": args.board, "mcu": chip["mcu"],
                "compiler": version, "optimization": args.optimization,
                "sdk": {"directory": str(sdk), "gpio_driver_sha256": hashlib.sha256(
                    (hal / "Src/stm32f4xx_hal_gpio.c").read_bytes()).hexdigest()},
                "sdk_sources": {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in sdk_sources},
                "resources": resources, "artifacts": artifacts,
                "sha256": {name: hashlib.sha256((output / name).read_bytes()).hexdigest()
                           for name in artifacts}}
    (output / "firmware.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(size_text.strip())
    print(output / "firmware.elf")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--generated-dir", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--board", default="nucleo-f446re")
    parser.add_argument("--cc", help="arm-none-eabi-gcc executable or its bin directory (ARM_GCC)")
    parser.add_argument("--sdk-dir", type=Path, help="STM32CubeF4 SDK root (STM32CUBE_F4)")
    parser.add_argument("--source", action="append", type=Path, default=[])
    parser.add_argument("--link-arg", action="append", default=[], help="extra linker argument (repeatable)")
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    try:
        build(parser.parse_args())
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
