"""Build the generated C ABI as an ESP-IDF application, with SDK-owned startup."""
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

from firmware.build.artifact import include_directories
from firmware.build.support import sha256, write_json

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
COMMON = HERE.parents[1] / "common"
SDK_VERSION = "5.4.1"
SDK_REVISION = "4c2820d377d1375e787bcef612f0c32c1427d183"
PIN_MASK = 0x3c07ff  # Header GPIO0..10 and GPIO18..21; flash pins stay reserved.
DEFAULT_MAPPINGS = {"uart0": {"tx_pin": 21, "rx_pin": 20},
                    "spi1": {"sck_pin": 4, "miso_pin": 5, "mosi_pin": 6},
                    "i2c1": {"scl_pin": 1, "sda_pin": 0}}


def cmake_value(value):
    value = str(value)
    if any(c in value for c in (";", "\0", "\n", "\r")):
        raise ValueError("CMake paths/arguments cannot contain semicolons, NUL or newlines")
    equals = "="
    while "]" + equals + "]" in value:
        equals += "="
    return "[" + equals + "[" + value + "]" + equals + "]"


def validate(profile, chip, stack_bytes, guard_bytes):
    if (chip.get("id"), chip.get("cpu"), chip.get("idf_target")) != ("esp32c3", "rv32imc", "esp32c3"):
        raise ValueError("ESP-IDF port requires ESP32-C3 / RV32IMC")
    for owner, fields in ((profile, {"flash_bytes": 4194304, "gpio_pin_mask": PIN_MASK}),
                          (chip, {"ram_bytes": 409600, "gpio_pin_mask": 0x3fffff, "analog_pin_mask": 63})):
        if any(type(owner.get(k)) is not int or owner[k] != v for k, v in fields.items()):
            raise ValueError("invalid ESP32-C3 board memory or pin mask")
    if (profile.get("clock") != {"source": "sdk", "frequency_hz": 160000000}
            or type(profile["clock"]["frequency_hz"]) is not int):
        raise ValueError("ESP32-C3 port requires the IDF 160 MHz clock")
    if (type(stack_bytes) is not int or type(guard_bytes) is not int
            or stack_bytes < 4096 or stack_bytes % 16 or guard_bytes < 32 or guard_bytes % 16
            or stack_bytes + guard_bytes >= chip["ram_bytes"]):
        raise ValueError("stack must be at least 4096, guard at least 32, both 16-byte aligned and within RAM")
    mappings = profile.get("peripherals")
    if not isinstance(mappings, dict) or mappings.keys() - DEFAULT_MAPPINGS.keys():
        raise ValueError("ESP32-C3 maps only uart0, spi1 and i2c1")
    for name, pins in mappings.items():
        if (not isinstance(pins, dict) or pins.keys() != DEFAULT_MAPPINGS[name].keys()
                or any(type(pin) is not int or pin < 0 or pin > 21 or not PIN_MASK & (1 << pin) for pin in pins.values())
                or len(set(pins.values())) != len(pins)):
            raise ValueError("invalid ESP32-C3 peripheral mapping: " + name)


def configure(output, profile, chip, stack_bytes, guard_bytes):
    validate(profile, chip, stack_bytes, guard_bytes)
    header = ["/* Generated from the board and chip profiles. */", "#pragma once",
              f"#define EMELA_GPIO_PIN_MASK {profile['gpio_pin_mask']:#x}U",
              f"#define EMELA_ANALOG_PIN_MASK {chip['analog_pin_mask']:#x}U",
              f"#define EMELA_STACK_BYTES {stack_bytes}", f"#define EMELA_STACK_GUARD_BYTES {guard_bytes}"]
    header += [f"#define EMELA_{name.upper()}_{key.upper()} {pin}"
               for name, pins in profile["peripherals"].items() for key, pin in pins.items()]
    (output / "board_config.h").write_text("\n".join(header) + "\n")


def sdk_path(args):
    sdk = Path(args.sdk_dir or os.environ.get("IDF_PATH")
               or ROOT / f".local/embedded-sdk/esp-idf-v{SDK_VERSION}").expanduser().resolve()
    version_file = sdk / "tools/cmake/version.cmake"
    if not (sdk / "tools/cmake/project.cmake").is_file() or not version_file.is_file():
        raise ValueError("ESP-IDF required; run tools/setup/esp32-c3-devkitm-1-n4x.sh or set IDF_PATH")
    content = version_file.read_text()
    parts = [re.search(r"set\(IDF_VERSION_" + key + r"\s+(\d+)\)", content) for key in ("MAJOR", "MINOR", "PATCH")]
    if not all(parts) or ".".join(p[1] for p in parts) != SDK_VERSION:
        raise ValueError(f"ESP-IDF {SDK_VERSION} required: {version_file}")
    return sdk


def sdk_identity(sdk):
    if not (sdk / ".git").exists():
        return {"source_kind": "archive", "revision": None}
    def git(*arguments):
        return subprocess.check_output(["git", "-C", str(sdk), *arguments], text=True,
                                       stderr=subprocess.STDOUT, timeout=60).strip()
    revision = git("rev-parse", "HEAD")
    if revision != SDK_REVISION:
        raise ValueError("ESP-IDF checkout must be at " + SDK_REVISION)
    if git("status", "--porcelain", "--untracked-files=no"):
        raise ValueError("ESP-IDF has tracked modifications or mismatched submodules")
    submodules = git("submodule", "status", "--recursive")
    if any(line.startswith(("-", "+", "U")) for line in submodules.splitlines()):
        raise ValueError("ESP-IDF submodules must be initialized at their pinned revisions")
    return {"source_kind": "git", "revision": revision, "submodules": submodules.splitlines(), "dirty": False}


def compiler_path(value):
    path = Path(value or os.environ.get("RISCV32_ESP_GCC") or "riscv32-esp-elf-gcc").expanduser()
    if path.is_dir():
        path /= "riscv32-esp-elf-gcc"
    found = shutil.which(str(path))
    if not found:
        raise ValueError("riscv32-esp-elf-gcc required; source the ESP-IDF environment or pass --cc")
    return Path(found).resolve()


def reject_tree_input(tree, path):
    path = Path(path).expanduser().absolute()
    if any(p == tree or tree in p.parents for p in (path, path.resolve())):
        raise ValueError("build input overlaps SDK build directory: " + str(path))


def elf_layout(path):
    data = path.read_bytes()
    if len(data) < 52 or data[:7] != b"\x7fELF\x01\x01\x01":
        raise ValueError("expected an ELF32 little-endian ESP32-C3 executable")
    fields = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    kind, machine, _, entry, phoff, shoff, _, _, phsize, phnum, shsize, shnum, shstr = fields
    if (kind != 2 or machine != 243 or shsize != 40 or shstr >= shnum
            or shoff + shsize * shnum > len(data) or phoff + phsize * phnum > len(data)):
        raise ValueError("invalid RISC-V executable or truncated ELF tables")
    headers = [struct.unpack_from("<10I", data, shoff + i * shsize) for i in range(shnum)]
    table = headers[shstr]
    if table[4] + table[5] > len(data):
        raise ValueError("truncated ELF section names")
    names = data[table[4]:table[4] + table[5]]
    sections = []
    for name, kind, flags, address, offset, size, *_ in headers:
        if flags & 2 and size:
            if name >= len(names) or (kind != 8 and offset + size > len(data)):
                raise ValueError("truncated ELF section")
            sections.append({"name": names[name:].split(b"\0", 1)[0].decode(),
                             "address": address, "size": size, "nobits": kind == 8})
    return entry, sections


def inspect_elf(output, profile, chip, cc, stack, guard, run):
    prefix = str(cc).removesuffix("gcc")
    symbols, sizes = {}, {}
    for line in run([prefix + "nm", "-S", "--defined-only", output / "firmware.elf"]).splitlines():
        fields = line.split()
        if len(fields) in (3, 4):
            symbols[fields[-1]] = int(fields[0], 16)
            if len(fields) == 4:
                sizes[fields[-1]] = int(fields[1], 16)
    if {"app_main", "emela_embedded_start", "emela_board_completed", "application_storage"} - symbols.keys():
        raise ValueError("missing ESP-IDF application or Emela runtime symbols")
    if sizes.get("application_storage") != stack + guard:
        raise ValueError("ESP-IDF application stack/guard reservation mismatch")
    undefined = {line.split()[-1] for line in run([prefix + "nm", "--undefined-only", output / "firmware.elf"]).splitlines() if line.split()}
    # IDF's common linker command forces these archive anchors even when their
    # Xtensa/C++ implementations are absent. They have no live relocations;
    # the SDK linker still rejects unresolved application references.
    if undefined - {"start_app", "__cxx_fatal_exception"}:
        raise ValueError("ESP32-C3 firmware has unresolved symbols: " + ", ".join(sorted(undefined)))
    entry, sections = elf_layout(output / "firmware.elf")
    if not 0x4037c000 <= entry < 0x403e4000:
        raise ValueError("ESP32-C3 entry point is outside IRAM")
    static_ram = 0
    for section in sections:
        address, end = section["address"], section["address"] + section["size"]
        if (0x3fc80000 <= address < end <= 0x3fce4000 or 0x4037c000 <= address < end <= 0x403e4000):
            static_ram += section["size"]
        elif not (0x3c000000 <= address < end <= 0x3c000000 + profile["flash_bytes"]
                  or 0x42000000 <= address < end <= 0x42000000 + profile["flash_bytes"]
                  or 0x50000000 <= address < end <= 0x50002000):
            raise ValueError("ESP32-C3 section outside memory windows: " + section["name"])
    image = (output / "firmware.bin").read_bytes()
    if not image or image[0] != 0xe9 or len(image) > profile["flash_bytes"] or static_ram > chip["ram_bytes"]:
        raise ValueError("invalid ESP32-C3 flash image or RAM usage")
    resources = {"flash_bytes": len(image), "flash_capacity": profile["flash_bytes"],
                 "static_ram_bytes": static_ram, "ram_capacity": chip["ram_bytes"],
                 "stack_reserved_bytes": stack, "stack_guard_bytes": guard,
                 "stack_guard_protection": "reserved-pattern-and-freertos-canary",
                 "system_allocator": True, "entry_point": entry, "sections": sections,
                 "sdk_undefined_archive_anchors": sorted(undefined)}
    write_json(output / "symbols.json", symbols)
    write_json(output / "resources.json", resources)
    (output / "size.txt").write_text(run([prefix + "size", output / "firmware.elf"]))
    return resources


def build(args, profile, chip, board_dir, inputs, metadata, run):
    validate(profile, chip, args.stack_bytes, args.guard_bytes)
    if any(export.get("c_symbol") == "app_main" for export in metadata.get("exports", [])):
        raise ValueError("ESP-IDF owns app_main; choose another C export symbol (for example emelino_app_main)")
    supported = {"service.gpio", "service.uart", "service.spi", "service.i2c", "runtime.defect", "runtime.lifecycle"}
    if set(metadata["services"]) - supported:
        raise ValueError("ESP-IDF BSP does not implement: " + ", ".join(sorted(set(metadata["services"]) - supported)))
    drivers = {name.removeprefix("service.") for name in metadata["services"] if name.startswith("service.")}
    if "spi" in drivers:
        drivers.add("gpio")
    for driver, mapping in (("uart", "uart0"), ("spi", "spi1"), ("i2c", "i2c1")):
        if driver in drivers and mapping not in profile["peripherals"]:
            raise ValueError("board has no mapping for required service: " + driver)
    output = args.output_dir.resolve()
    tree = output / "sdk-build"
    if tree.is_symlink():
        raise ValueError("SDK build directory cannot be a symlink")
    sdk_input = Path(args.sdk_dir or os.environ.get("IDF_PATH") or ROOT / f".local/embedded-sdk/esp-idf-v{SDK_VERSION}")
    python = Path(os.environ.get("IDF_PYTHON_ENV_PATH", sys.prefix)) / "bin/python"
    if not python.is_file():
        python = Path(sys.executable)
    sources = [HERE / "platform.c", HERE / "pins.c", HERE / "time.c",
               *([COMMON / "spi.c"] if "spi" in drivers else []),
               *[HERE / (name + ".c") for name in sorted(drivers)],
               *([board_dir / "init.c"] if (board_dir / "init.c").is_file() else []),
               *inputs["source"], *[path.resolve() for path in args.source]]
    cc_input = args.cc or os.environ.get("RISCV32_ESP_GCC") or "riscv32-esp-elf-gcc"
    for path in (sdk_input, args.generated_dir, python, cc_input, board_dir, *sources, *args.source,
                 *(p for paths in inputs.values() for p in paths)):
        reject_tree_input(tree, path)
    compiler_input = Path(cc_input).expanduser()
    if compiler_input.is_dir():
        compiler_input /= "riscv32-esp-elf-gcc"
    found = shutil.which(str(compiler_input))
    if found:
        reject_tree_input(tree, found)
    cc = compiler_path(args.cc)
    reject_tree_input(tree, cc)
    sdk = sdk_path(args)
    identity = sdk_identity(sdk)
    for source in sources:
        if not source.is_file():
            raise ValueError("missing build input: " + str(source))
    includes = {output, HERE, COMMON, board_dir, args.generated_dir.resolve(), *include_directories(inputs)}
    configure(output, profile, chip, args.stack_bytes, args.guard_bytes)
    values = {"EMELINO_SOURCES": sources, "EMELINO_INCLUDES": sorted(includes), "EMELINO_OUTPUT": [output],
              "EMELINO_OPTIMIZATION": [args.optimization], "EMELINO_LINK_ARGS": args.link_arg}
    config = output / "emelino.cmake"
    config.write_text("\n".join("set(" + key + " " + " ".join(cmake_value(v) for v in value) + ")"
                                for key, value in values.items()) + "\n")
    # Refuse overlap above before removing this disposable, task-owned tree.
    if tree.exists():
        shutil.rmtree(tree)
    artifacts = ["firmware.elf", "firmware.bin", "firmware-app.bin", "bootloader.bin", "partition-table.bin",
                 "firmware.map", "sdkconfig", "resources.json", "symbols.json", "size.txt", "sdk-inputs.json", "flash-plan.json"]
    for name in [*artifacts, "compiler-path.txt"]:
        (output / name).unlink(missing_ok=True)
    environment = dict(os.environ, IDF_PATH=str(sdk), IDF_COMPONENT_MANAGER="0",
                       PATH=str(cc.parent) + os.pathsep + os.environ.get("PATH", ""))
    version = run([cc, "--version"]).splitlines()[0]
    run(["cmake", "-S", HERE, "-B", tree, "-G", "Ninja", "-DIDF_TARGET=esp32c3",
         "-DPYTHON=" + str(python), "-DEMELINO_CONFIG=" + str(config),
         "-DSDKCONFIG=" + str(output / "sdkconfig"), "-DSDKCONFIG_DEFAULTS=" + str(HERE / "sdkconfig.defaults"),
         "-DCCACHE_ENABLE=0"], env=environment, timeout=180)
    actual = Path((output / "compiler-path.txt").read_text().strip()).resolve()
    if actual != cc:
        raise ValueError("ESP-IDF selected a different compiler: " + str(actual))
    run(["cmake", "--build", tree, "--parallel", "2"], env=environment, timeout=600)
    for name, source in {"firmware.elf": "firmware.elf", "firmware.map": "firmware.map",
                         "firmware-app.bin": "firmware.bin", "bootloader.bin": "bootloader/bootloader.bin",
                         "partition-table.bin": "partition_table/partition-table.bin"}.items():
        shutil.copyfile(tree / source, output / name)
    flasher = json.loads((tree / "flasher_args.json").read_text())
    settings = flasher["flash_settings"]
    flash_files = sorted(flasher["flash_files"].items(), key=lambda item: int(item[0], 0))
    merge = [python, "-m", "esptool", "--chip", "esp32c3", "merge_bin", "-o", output / "firmware.bin",
             "--flash_size", "4MB", "--flash_mode", settings["flash_mode"], "--flash_freq", settings["flash_freq"]]
    plan = []
    for offset, relative in flash_files:
        source = (tree / relative).resolve()
        if tree not in source.parents or not source.is_file() or not 0 <= int(offset, 0) < profile["flash_bytes"]:
            raise ValueError("invalid ESP-IDF flash plan")
        merge += [offset, source]
        plan.append({"offset": int(offset, 0), "file": relative, "sha256": sha256(source)})
    run(merge, env=environment)
    write_json(output / "flash-plan.json", {"chip": "esp32c3", "flash_bytes": profile["flash_bytes"], "files": plan})
    resources = inspect_elf(output, profile, chip, cc, args.stack_bytes, args.guard_bytes, run)
    commands = json.loads((tree / "compile_commands.json").read_text())
    source_paths = {Path(item["file"]).resolve() for item in commands}
    hashes = {str(p): sha256(p) for p in sorted(source_paths | set(sources)) if p.is_file()}
    write_json(output / "sdk-inputs.json", {"sources_sha256": hashes,
        "sdk_version_file_sha256": sha256(sdk / "tools/cmake/version.cmake"),
        "defaults_sha256": sha256(HERE / "sdkconfig.defaults"), "sdkconfig_sha256": sha256(output / "sdkconfig")})
    return {"artifacts": artifacts, "compiler": {"path": str(cc), "version": version, "sha256": sha256(cc)},
            "sdk": {"name": "ESP-IDF", "version": SDK_VERSION, "path": str(sdk), **identity},
            "resources": resources, "flash_image_origin": 0, "app_image_origin": 0x10000}
