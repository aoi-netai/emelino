"""Pico SDK owns startup, CPU ABI and linking; emelino owns the board contract."""

import os
import hashlib
from pathlib import Path
import re
import shutil
import struct
import subprocess

from firmware.build.arm_gcc import compiler_path
from firmware.build.artifact import include_directories
from firmware.build.support import sha256, write_json

HERE = Path(__file__).resolve().parent
COMMON = HERE.parents[1] / "common"
ROOT = HERE.parents[2]
SDK_VERSION = "2.3.1"
SDK_REVISION = "079c6f39023649b154152db30f1d781e884879bc"
DEVICES = {
    "rp2040": ("pico", "rp2040", "cortex-m0plus", 264 * 1024, 2 * 1024 * 1024, 125000000),
    "rp2350a": ("pico2", "rp2350-arm-s", "cortex-m33", 520 * 1024, 4 * 1024 * 1024, 150000000),
}
MAPPINGS = {"uart0": {"tx_pin": 0, "rx_pin": 1},
            "spi1": {"sck_pin": 18, "miso_pin": 16, "mosi_pin": 19},
            "i2c1": {"scl_pin": 5, "sda_pin": 4}}
BOARD_PINS = sum(1 << pin for pin in [*range(23), 25, 26, 27, 28])


def validate(profile, chip, stack_bytes, guard_bytes):
    expected = DEVICES.get(chip["id"])
    if expected is None:
        raise ValueError("unsupported Pico MCU")
    board, platform, cpu, ram, flash, clock = expected
    if (profile.get("pico_board"), profile.get("pico_platform"), chip.get("cpu")) != (board, platform, cpu):
        raise ValueError("Pico board/platform/CPU mismatch")
    if (type(profile.get("flash_bytes")) is not int or profile["flash_bytes"] != flash
            or any(type(chip.get(key)) is not int for key in ("flash_origin", "ram_origin", "ram_bytes"))
            or chip.get("flash_origin") != 0x10000000 or chip.get("ram_origin") != 0x20000000
            or chip["ram_bytes"] != ram):
        raise ValueError("invalid Pico Flash/RAM layout")
    if (profile.get("clock") != {"source": "sdk", "frequency_hz": clock}
            or type(profile["clock"]["frequency_hz"]) is not int):
        raise ValueError("Pico port requires the official SDK board clock")
    if (type(stack_bytes) is not int or type(guard_bytes) is not int or stack_bytes <= 0
            or stack_bytes % 8 or guard_bytes < 32 or guard_bytes % 8):
        raise ValueError("stack must be positive and guard at least 32; both must be multiples of 8")
    if stack_bytes + guard_bytes >= ram:
        raise ValueError("stack/guard reservation exceeds RAM")
    if (type(profile.get("gpio_pin_mask")) is not int or profile["gpio_pin_mask"] != BOARD_PINS
            or any(type(chip.get(key)) is not int for key in ("gpio_pin_mask", "analog_pin_mask"))
            or chip.get("gpio_pin_mask") != (1 << 30) - 1
            or chip.get("analog_pin_mask") != 0x3c000000):
        raise ValueError("invalid Pico board/package pin masks")
    mappings = profile.get("peripherals", {})
    if not isinstance(mappings, dict) or mappings.keys() - MAPPINGS.keys():
        raise ValueError("Pico port maps only uart0, spi1 and i2c1")
    for name, mapping in mappings.items():
        if (mapping != MAPPINGS[name] or not isinstance(mapping, dict)
                or any(type(pin) is not int for pin in mapping.values())):
            raise ValueError("unsupported Pico peripheral pin mapping: " + name)


def configure(output, profile, chip, stack_bytes, guard_bytes, timed=False):
    validate(profile, chip, stack_bytes, guard_bytes)
    header = ["/* Generated from the board and MCU profiles. */",
              "#ifndef EMELINO_PICO_BOARD_CONFIG_H", "#define EMELINO_PICO_BOARD_CONFIG_H",
              f"#define EMELA_GPIO_PIN_MASK {profile['gpio_pin_mask']:#x}U",
              f"#define EMELA_ANALOG_PIN_MASK {chip['analog_pin_mask'] & profile['gpio_pin_mask']:#x}U",
              f"#define EMELA_BOARD_CLOCK_HZ {profile['clock']['frequency_hz']}U"]
    header += [f"#define EMELA_{name.upper()}_{key.upper()} {value}"
               for name, pins in profile.get("peripherals", {}).items() for key, value in pins.items()]
    if timed:
        header.append("#define EMELA_PERIPHERAL_TIME 1")
    (output / "board_config.h").write_text("\n".join(header + ["#endif", ""]))
    # SDK's default SCRATCH_Y is only 4 KiB. For this core-0-only port, reserve
    # the requested stack at the top of contiguous SRAM, with an unallocated
    # guard below it. SDK's sections/startup stay unchanged; core 1 gets no RAM.
    ram_end = chip["ram_origin"] + chip["ram_bytes"]
    stack_bottom = ram_end - stack_bytes
    guard_start = stack_bottom - guard_bytes
    linker = output / "linker"
    linker.mkdir(exist_ok=True)
    values = {"RAM_ORIGIN": chip["ram_origin"], "RAM_LENGTH": guard_start - chip["ram_origin"],
              "SCRATCH_X_ORIGIN": guard_start, "SCRATCH_X_LENGTH": 0,
              "SCRATCH_Y_ORIGIN": stack_bottom, "SCRATCH_Y_LENGTH": stack_bytes,
              "__emela_stack_guard_start__": guard_start, "__emela_stack_guard_end__": stack_bottom}
    (linker / "set_memory_locations.incl").write_text(
        "/* SDK linker overrides: core 0 stack and reserved guard, no core 1. */\n"
        + "".join(f"{key} = {value:#x};\n" for key, value in values.items())
        + "XIP_RAM_ORIGIN = XIP_RAM_ORIGIN_DEFAULT;\nXIP_RAM_LENGTH = XIP_RAM_LENGTH_DEFAULT;\n")


def cmake_value(value):
    """Bracket-quote literal paths/arguments, including whitespace and CMake syntax."""
    value = str(value)
    if ";" in value or "\0" in value or "\n" in value:
        raise ValueError("CMake paths/arguments cannot contain semicolons, NUL or newlines")
    equals = "="
    while "]" + equals + "]" in value:
        equals += "="
    return "[" + equals + "[" + value + "]" + equals + "]"


def sdk_directories(args):
    """Locate configured inputs without dereferencing their symlinks."""
    sdk = Path(args.sdk_dir or os.environ.get("PICO_SDK_PATH")
               or ROOT / f".local/embedded-sdk/pico-sdk-{SDK_VERSION}").expanduser().absolute()
    picotool = Path(os.environ.get("picotool_DIR") or ROOT / f".local/picotool-{SDK_VERSION}/picotool").expanduser().absolute()
    return sdk, picotool


def sdk_paths(args):
    sdk, picotool = (path.resolve() for path in sdk_directories(args))
    if not (sdk / "pico_sdk_init.cmake").is_file():
        raise ValueError("Pico SDK required; run tools/setup/pico-sdk.sh or set PICO_SDK_PATH")
    version_text = (sdk / "pico_sdk_version.cmake").read_text()
    parts = [re.search(r"set\(PICO_SDK_VERSION_" + field + r"\s+(\d+)\)", version_text)
             for field in ("MAJOR", "MINOR", "REVISION")]
    if not all(parts):
        raise ValueError("malformed Pico SDK version file: " + str(sdk / "pico_sdk_version.cmake"))
    version = ".".join(part[1] for part in parts)
    if version != SDK_VERSION:
        raise ValueError(f"Pico SDK {SDK_VERSION} required (found {version})")
    sdk_identity(sdk)
    if not (picotool / "picotoolConfig.cmake").is_file():
        raise ValueError("installed picotool required; run tools/setup/pico-sdk.sh or set picotool_DIR")
    return sdk, picotool


def reject_build_tree_input(tree, path):
    # Check the link location as well as its target: cleanup must preserve both.
    path = Path(path).expanduser().absolute()
    if any(candidate == tree or tree in candidate.parents for candidate in (path, path.resolve())):
        raise ValueError(f"build input overlaps SDK build directory: {path}")


def sdk_identity(sdk):
    """Managed checkouts must match the pin; archives are identified by file hashes."""
    if not (sdk / ".git").exists():
        return {"revision": None, "source_kind": "archive", "dirty": None}
    def git(*arguments):
        return subprocess.check_output(["git", "-C", str(sdk), *arguments], text=True,
                                       stderr=subprocess.STDOUT, timeout=30).strip()
    revision = git("rev-parse", "HEAD")
    if revision != SDK_REVISION:
        raise ValueError("Pico SDK checkout must be at " + SDK_REVISION)
    if git("status", "--porcelain", "--untracked-files=no"):
        raise ValueError("Pico SDK checkout has tracked modifications; use a clean SDK")
    return {"revision": revision, "source_kind": "git", "dirty": False}


def sdk_source_hashes(sdk):
    """Record source content, also making SDK archives and extra local headers visible."""
    hashes = {}
    for directory, children, files in os.walk(sdk):
        children[:] = sorted(name for name in children if name != ".git")
        for name in sorted(files):
            if name == ".git":
                continue  # A worktree's Git metadata is a file.
            path = Path(directory) / name
            if path.is_file():
                hashes[path.relative_to(sdk).as_posix()] = sha256(path)
    return hashes


def elf_layout(path):
    """Read ELF32 little-endian alloc sections without external Python packages."""
    data = path.read_bytes()
    if len(data) < 52 or data[:7] != b"\x7fELF\x01\x01\x01":
        raise ValueError("expected an ELF32 little-endian firmware")
    fields = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    kind, machine, _, entry, phoff, shoff, _, _, phsize, phnum, shsize, shnum, shstr = fields
    if kind != 2 or machine != 40 or phsize != 32 or shsize != 40 or shstr >= shnum:
        raise ValueError("expected an executable Arm ELF")
    if phoff + phsize * phnum > len(data) or shoff + shsize * shnum > len(data):
        raise ValueError("truncated ELF tables")
    headers = [struct.unpack_from("<10I", data, shoff + i * shsize) for i in range(shnum)]
    names_header = headers[shstr]
    names = data[names_header[4]:names_header[4] + names_header[5]]
    sections = []
    for name, kind, flags, address, offset, size, *_ in headers:
        if flags & 2 and size:
            sections.append({"name": names[name:].split(b"\0", 1)[0].decode(), "address": address,
                             "size": size, "nobits": kind == 8, "offset": offset, "flags": flags})
    segments = [struct.unpack_from("<8I", data, phoff + i * phsize) for i in range(phnum)]
    return data, entry, sections, segments


def inspect_elf(args, profile, chip, cc, run):
    output = args.output_dir.resolve()
    elf = output / "firmware.elf"
    prefix = str(cc).removesuffix("gcc")
    symbols = {}
    for line in run([prefix + "nm", "-S", "--defined-only", elf]).splitlines():
        fields = line.split()
        if len(fields) in (3, 4):
            symbols[fields[-1]] = int(fields[0], 16)
    if run([prefix + "nm", "--undefined-only", elf]).strip():
        raise ValueError("Pico firmware has unresolved symbols")
    allocator = re.compile(r"(?:__wrap_)?(?:malloc|calloc|realloc|free|sbrk|_sbrk|_sbrk_r|_malloc_r|_calloc_r|_realloc_r|_free_r)")
    if any(allocator.fullmatch(name) for name in symbols):
        raise ValueError("firmware unexpectedly links a system allocator")
    attributes = run([prefix + "readelf", "-A", elf])
    expected_arch = "v6S-M" if chip["id"] == "rp2040" else "v8-M.mainline"
    if not re.search(r"Tag_CPU_arch:\s+" + re.escape(expected_arch) + r"(?:\s|$)", attributes):
        raise ValueError("ELF CPU attributes do not match the board")
    data, entry, sections, segments = elf_layout(elf)
    flash_start, ram_start = chip["flash_origin"], chip["ram_origin"]
    flash_end, ram_end = flash_start + profile["flash_bytes"], ram_start + chip["ram_bytes"]
    guard_start, stack_bottom = ram_end - args.stack_bytes - args.guard_bytes, ram_end - args.stack_bytes
    required = {"main", "emela_embedded_start", "emela_board_completed", "__vectors", "__StackTop", "__StackBottom"}
    if required - symbols.keys():
        raise ValueError("missing Pico runtime symbols: " + ", ".join(sorted(required - symbols.keys())))
    if (symbols["__StackTop"] != ram_end or symbols["__StackBottom"] != stack_bottom
            or symbols.get("__emela_stack_guard_start__") != guard_start
            or symbols.get("__emela_stack_guard_end__") != stack_bottom):
        raise ValueError("SDK stack/guard reservation mismatch")
    static_ram = 0
    for section in sections:
        address, end = section["address"], section["address"] + section["size"]
        if ram_start <= address < ram_end:
            if section["name"] == ".stack_dummy":
                if (address, end) != (stack_bottom, ram_end):
                    raise ValueError("invalid SDK stack section")
            elif end <= guard_start:
                static_ram += section["size"]
            else:
                raise ValueError("Pico section overlaps stack/guard: " + section["name"])
        elif not flash_start <= address < end <= flash_end:
            raise ValueError("Pico section outside board memory: " + section["name"])
    for kind, offset, vaddr, physical, filesz, memsz, flags, align in segments:
        if kind == 1 and filesz and not flash_start <= physical < physical + filesz <= flash_end:
            raise ValueError("Pico load segment outside Flash")
    image = (output / "firmware.bin").read_bytes()
    vector_offset = symbols["__vectors"] - flash_start
    if not 0 <= vector_offset <= len(image) - 8:
        raise ValueError("Pico vector table is missing from Flash image")
    sp, reset = struct.unpack_from("<II", image, vector_offset)
    if (sp != ram_end or not reset & 1 or not flash_start <= (reset & ~1) < flash_start + len(image)
            or not entry & 1 or not flash_start <= (entry & ~1) < flash_start + len(image)):
        raise ValueError("invalid Pico entry/SP/reset vector")
    if not 8 <= len(image) <= profile["flash_bytes"]:
        raise ValueError("Pico image exceeds Flash")
    resources = {"flash_bytes": len(image), "flash_capacity": profile["flash_bytes"],
                 "static_ram_bytes": static_ram, "ram_capacity": chip["ram_bytes"],
                 "application_ram_capacity": guard_start - ram_start,
                 "stack_reserved_bytes": args.stack_bytes, "stack_guard_bytes": args.guard_bytes,
                 "stack_guard_protection": "reserved-only", "system_allocator": False,
                 "entry_point": entry, "vector_table": symbols["__vectors"], "sections": sections}
    write_json(output / "symbols.json", symbols)
    write_json(output / "resources.json", resources)
    (output / "size.txt").write_text(run([prefix + "size", elf]))
    return resources


def build(args, profile, chip, board_dir, inputs, metadata, run):
    output = args.output_dir.resolve()
    supported = {"service.gpio", "service.uart", "service.spi", "service.i2c", "runtime.defect", "runtime.lifecycle"}
    if set(metadata["services"]) - supported:
        raise ValueError("Pico BSP does not implement: " + ", ".join(sorted(set(metadata["services"]) - supported)))
    drivers = {name.removeprefix("service.") for name in metadata["services"] if name.startswith("service.")}
    if "spi" in drivers:
        drivers.add("gpio")
    timed = bool(drivers & {"uart", "spi", "i2c"})
    validate(profile, chip, args.stack_bytes, args.guard_bytes)
    for driver, mapping in (("uart", "uart0"), ("spi", "spi1"), ("i2c", "i2c1")):
        if driver in drivers and mapping not in profile.get("peripherals", {}):
            raise ValueError("board has no mapping for required service: " + driver)
    sources = [HERE / "platform.c", HERE / "pins.c",
               *([board_dir / "init.c"] if (board_dir / "init.c").is_file() else []),
               *([HERE / "time.c"] if timed else []), *([COMMON / "spi.c"] if "spi" in drivers else []),
               *[HERE / (name + ".c") for name in sorted(drivers)], *inputs["source"],
               *[path.resolve() for path in args.source]]
    # The disposable SDK tree must never contain caller-owned inputs or tools.
    # Do this before writing configuration, removing artifacts, or inspecting Git.
    tree = output / "sdk-build"
    if tree.is_symlink():
        raise ValueError("SDK build directory cannot be a symlink")
    sdk, picotool = sdk_directories(args)
    for path in (args.generated_dir, sdk, picotool, board_dir, *sources, *args.source,
                 *(path for paths in inputs.values() for path in paths)):
        reject_build_tree_input(tree, path)
    compiler_input = Path(args.cc or os.environ.get("ARM_GCC") or "arm-none-eabi-gcc").expanduser()
    reject_build_tree_input(tree, compiler_input)
    if compiler_input.is_dir():
        compiler_input /= "arm-none-eabi-gcc"
    found_compiler = shutil.which(str(compiler_input))
    if found_compiler:
        reject_build_tree_input(tree, found_compiler)
    cc = compiler_path(args.cc)
    reject_build_tree_input(tree, cc)
    sdk, picotool = sdk_paths(args)
    for source in sources:
        if not source.is_file():
            raise ValueError(f"missing build input: {source}")
    configure(output, profile, chip, args.stack_bytes, args.guard_bytes, timed)
    includes = {output, HERE, COMMON, board_dir, args.generated_dir.resolve(), *include_directories(inputs)}
    values = {"EMELINO_SOURCES": sources, "EMELINO_INCLUDES": sorted(includes),
              "EMELINO_OUTPUT": [output], "EMELINO_OPTIMIZATION": [args.optimization],
              "EMELINO_STACK_BYTES": [args.stack_bytes], "EMELINO_FLASH_BYTES": [profile["flash_bytes"]],
              "EMELINO_LIBRARIES": ["hardware_" + name for name in sorted(drivers & {"uart", "spi", "i2c"})],
              "EMELINO_LINK_ARGS": args.link_arg}
    config = output / "emelino.cmake"
    config.write_text("\n".join("set(" + key + " " + " ".join(cmake_value(v) for v in value) + ")"
                                for key, value in values.items()) + "\n")
    # A new SDK build tree prevents cross-board/toolchain/config cache reuse.
    if tree.exists():
        shutil.rmtree(tree)
    # A successful SDK command must not accidentally certify an earlier binary.
    for name in ("firmware.elf", "firmware.bin", "firmware.uf2", "firmware.map", "firmware.elf.map",
                 "resources.json", "symbols.json", "size.txt", "sdk-inputs.json",
                 "picotool-path.txt", "compiler-path.txt"):
        (output / name).unlink(missing_ok=True)
    version = run([cc, "--version"]).splitlines()[0]
    cmake = shutil.which("cmake")
    if not cmake:
        raise ValueError("CMake required; run tools/setup/pico-sdk.sh")
    cmake_version = run([cmake, "--version"]).splitlines()[0]
    run(["cmake", "-S", HERE, "-B", tree, "-G", "Unix Makefiles",
         "-DEMELINO_CONFIG=" + str(config), "-DPICO_SDK_PATH=" + str(sdk),
         "-DPICO_BOARD=" + profile["pico_board"], "-DPICO_PLATFORM=" + profile["pico_platform"],
         "-DPICO_TOOLCHAIN_PATH=" + str(cc.parent.parent), "-Dpicotool_DIR=" + str(picotool),
         "-DCMAKE_BUILD_TYPE=Debug", "-DFETCHCONTENT_FULLY_DISCONNECTED=ON",
         "-DPICO_SDK_FETCH_FROM_GIT=OFF", "-DPICOTOOL_FORCE_FETCH_FROM_GIT=OFF"], timeout=120)
    actual_compiler = Path((output / "compiler-path.txt").read_text().strip()).resolve()
    if actual_compiler != cc:
        raise ValueError(f"SDK selected {actual_compiler}, but --cc/ARM_GCC requested {cc}")
    picotool_binary = Path((output / "picotool-path.txt").read_text().strip()).resolve()
    picotool_version = run([picotool_binary, "version"]).splitlines()[0]
    if not re.match(r"picotool v2\.3\.1(?:\s|$)", picotool_version):
        raise ValueError("picotool binary version does not match the pinned 2.3.1 package")
    run(["cmake", "--build", tree, "--parallel", "2"], timeout=240)
    if (tree / "firmware.uf2").is_file():
        shutil.copyfile(tree / "firmware.uf2", output / "firmware.uf2")
    # SDK emits the map beside the target by default; normalize if needed.
    if (output / "firmware.elf.map").is_file():
        shutil.copyfile(output / "firmware.elf.map", output / "firmware.map")
    elif (tree / "firmware.elf.map").is_file():
        shutil.copyfile(tree / "firmware.elf.map", output / "firmware.map")
    resources = inspect_elf(args, profile, chip, cc, run)
    identity = sdk_identity(sdk)
    sdk_hashes = sdk_source_hashes(sdk)
    write_json(output / "sdk-inputs.json", sdk_hashes)
    tree_hash = hashlib.sha256("".join(name + "\0" + value + "\n"
        for name, value in sorted(sdk_hashes.items())).encode()).hexdigest()
    return {"compiler": version, "resources": resources,
            "tools": {"cmake": {"version": cmake_version, "executable": cmake, "sha256": sha256(cmake)},
                      "picotool": {"version": picotool_version, "executable": str(picotool_binary),
                                   "sha256": sha256(picotool_binary)}},
            "sdk": {"name": "Raspberry Pi Pico SDK", "version": SDK_VERSION, **identity,
                    "source_tree_sha256": tree_hash,
                    "directory": str(sdk), "board": profile["pico_board"], "platform": profile["pico_platform"]},
            "sdk_sources": {"pico_sdk_init.cmake": sha256(sdk / "pico_sdk_init.cmake")},
            "artifacts": ["firmware.elf", "firmware.bin", "firmware.uf2", "firmware.map",
                          "resources.json", "symbols.json", "size.txt", "build.log", "board_config.h", "emelino.cmake",
                          "linker/set_memory_locations.incl", "sdk-inputs.json"]}
