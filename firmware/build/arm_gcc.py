"""Shared Cortex-M4F GCC compilation, linking and firmware validation."""

import os
from pathlib import Path
import re
import shutil
import struct

from firmware.build.support import write_json

CORTEX_M4F = Path(__file__).resolve().parents[1] / "common/cortex_m4f"


def compiler_path(value):
    value = value or os.environ.get("ARM_GCC") or "arm-none-eabi-gcc"
    path = Path(value).expanduser()
    if path.is_dir():
        path /= "arm-none-eabi-gcc"
    found = shutil.which(str(path))
    if not found:
        raise ValueError("ARM GCC not found; pass --cc or set ARM_GCC")
    return Path(found).resolve()


def configure(output, chip, stack_bytes, guard_bytes):
    vectors = ["/* IRQ slots from the MCU's CMSIS vector table. */"]
    vectors += ["    vector " + name if name else "    .word 0"
                for name in chip["external_vectors"]]
    (output / "board_vectors.inc").write_text("\n".join(vectors) + "\n")
    linker = (CORTEX_M4F / "firmware.ld.in").read_text()
    values = {"FLASH_ORIGIN": chip["flash_origin"], "FLASH_BYTES": chip["flash_bytes"],
              "RAM_ORIGIN": chip["ram_origin"], "RAM_BYTES": chip["ram_bytes"],
              "STACK_BYTES": stack_bytes, "GUARD_BYTES": guard_bytes}
    for key, value in values.items():
        linker = linker.replace("@" + key + "@", hex(value))
    if re.search(r"@[A-Z_]+@", linker):
        raise ValueError("unresolved linker template placeholder")
    (output / "firmware.ld").write_text(linker)


def build(args, chip, sources, includes, defines, run):
    output = args.output_dir.resolve()
    cc = compiler_path(args.cc)
    prefix = str(cc).removesuffix("gcc")
    for source in sources:
        if not source.is_file():
            raise ValueError(f"missing build input: {source}")
    arch = ["-mcpu=" + chip["cpu"], "-mthumb", "-mfpu=" + chip["fpu"],
            "-mfloat-abi=" + chip["float_abi"]]
    flags = arch + ["-" + args.optimization, "-g3", "-std=c11", "-ffunction-sections",
                    "-fdata-sections", "-Wall", "-Wextra", "-Werror", "-fstack-usage",
                    *["-D" + value for value in defines]]
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
    return {"compiler": version, "resources": resources,
            "artifacts": ["firmware.elf", "firmware.bin", "firmware.map", "resources.json", "symbols.json"]}
