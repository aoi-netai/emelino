"""ESP32-C3 smoke test with pinned external models and explicit bootstrap hooks."""
import json
import os
from pathlib import Path
import re
import shutil

from firmware.build.support import sha256
from support.common import ROOT, build_artifacts, renode_workspace, run, value

HERE = Path(__file__).resolve().parent


class Platform:
    def __init__(self, target):
        self.target = target


def dependencies(args):
    dependency = json.loads((HERE / "dependency.json").read_text())
    models = Path(args.esp32c3_models_dir or os.environ.get("ESP32C3_RENODE_MODELS")
                  or ROOT / dependency["directory"]).expanduser().resolve()
    if not (models / "platforms/cpus/esp32c3.repl").is_file():
        raise ValueError("ESP32-C3 models required; run tools/setup/esp32c3-renode.sh or pass --esp32c3-models-dir")
    status, revision = run(["git", "-C", models, "rev-parse", "HEAD"])
    if status or revision.strip() != dependency["revision"]:
        raise ValueError("ESP32-C3 Renode models must match dependency.json")
    status, modified = run(["git", "-C", models, "status", "--porcelain", "--untracked-files=no"])
    if status or modified.strip():
        raise ValueError("ESP32-C3 Renode models have tracked modifications")
    tools = Path(os.environ.get("IDF_TOOLS_PATH", ROOT / ".local/espressif"))
    rom = Path(args.esp32c3_rom or os.environ.get("ESP32C3_ROM_ELF")
               or tools / "tools/esp-rom-elfs" / dependency["rom_release"] / dependency["rom_filename"]).expanduser().resolve()
    if not rom.is_file() or sha256(rom) != dependency["rom_sha256"]:
        raise ValueError("ESP32-C3 ROM must match Espressif release " + dependency["rom_release"] + "; pass --esp32c3-rom")
    return models, rom, dependency


def bootstrap_script(models, alias, temporary, manifest, symbols):
    """Use upstream's ROM/CLIC bootstrap, with addresses from this ELF.

    This is deliberately not labeled raw-flash boot: both the ROM-main redirect
    and interrupt dispatch hook are required by the pinned external model.
    """
    source = (models / "hello_world/setup.resc").read_text()
    includes = re.findall(r"^include \$base/(peripherals/[a-z0-9-]+/ESP32C3_[A-Za-z]+\.cs)$", source, re.M)
    if len(includes) != 16:
        raise ValueError("unexpected ESP32-C3 model include list")
    for required in ("_vector_table", "_interrupt_handler", "esp32c3_test_done", "esp32c3_test_checks",
                     "esp32c3_test_failures", "esp32c3_test_last_failed", "emela_board_failures", "emela_board_completed"):
        if required not in symbols:
            raise ValueError("missing ESP32-C3 smoke symbol: " + required)
    drom = [s["address"] for s in manifest["resources"]["sections"]
            if not s["nobits"] and 0x3c000000 <= s["address"] < 0x3c400000]
    if not drom or min(drom) & 0xffff != 0x20:
        raise ValueError("unexpected ESP32-C3 application flash layout")
    # One compilation unit avoids recompiling the Renode framework references
    # sixteen times. Preserve the pinned models' namespace/type definitions.
    usings, bodies = set(), []
    for path in includes:
        content = (models / path).read_text()
        usings.update(re.findall(r"^using [^\n]+;$", content, re.M))
        bodies.append(re.sub(r"^using [^\n]+;\n", "", content, flags=re.M))
    (Path(temporary) / "models.cs").write_text("\n".join(sorted(usings)) + "\n" + "\n".join(bodies))
    script = ["using sysbus", f"include @{temporary}/models.cs",
              'mach create "esp32c3"', f"machine LoadPlatformDescription @{temporary}/models/platforms/cpus/esp32c3.repl",
              f"sysbus.uart0 CreateFileBackend @{alias}/uart.log true", f"sysbus LoadELF @{temporary}/rom.elf",
              f"sysbus LoadELF @{alias}/firmware/firmware.elf",
              f"sysbus LoadBinary @{alias}/firmware/firmware.bin 0x3c000000",
              f"sysbus LoadBinary @{alias}/firmware/firmware-app.bin {min(drom) - 0x20:#x}",
              f"sysbus LoadBinary @{temporary}/models/platforms/rom_data_segment.bin 0x3ff00000",
              f"sysbus LoadBinary @{temporary}/models/platforms/rom_iram_data.bin 0x40059590",
              "cpu PC 0x40001e90", "set redirect_rom_main", '"""',
              "from Antmicro.Renode.Peripherals.CPU import RegisterValue",
              f"cpu.PC = RegisterValue.Create({manifest['resources']['entry_point']:#x}, 32)", '"""',
              "cpu AddHook 0x40047e3c $redirect_rom_main", "set cycle_counter", '"""',
              "if request.IsRead:", "    request.Value = cpu.ExecutedInstructions", '"""',
              "cpu RegisterCSRHandlerFromString 0x802 $cycle_counter", "start", "sleep 1", "pause",
              f"cpu MTVEC {symbols['_vector_table'] | 3:#x}", "set dispatch_clic", '"""',
              "from Antmicro.Renode.Peripherals.CPU import RegisterValue", "if cpu.MCAUSE.RawValue & 0x80000000:",
              f"    cpu.PC = RegisterValue.Create({symbols['_interrupt_handler']:#x}, 32)", '"""',
              f"cpu AddHook {symbols['_vector_table']:#x} $dispatch_clic"]
    for line in range(32):
        address = 0x20001000 + line * 4
        script += [f"sysbus WriteByte {address + 1:#x} 1", f"sysbus WriteByte {address + 2:#x} 0",
                   f"sysbus WriteByte {address + 3:#x} 0xff"]
    script += ["cpu WfiAsNop true", "start", "sleep 4", "pause"]
    for name in ("esp32c3_test_done", "esp32c3_test_checks", "esp32c3_test_failures", "esp32c3_test_last_failed",
                 "emela_board_failures", "emela_board_completed", "emela_board_boot_initialized"):
        script += [f'echo "{name}"', f"sysbus ReadDoubleWord {symbols[name]:#x}"]
    script += ['echo "gpio_output"', "sysbus ReadDoubleWord 0x60004004",
               'echo "gpio_enable"', "sysbus ReadDoubleWord 0x60004020", "cpu PC", "cpu MCAUSE"]
    if "g_panic_abort_details" in symbols:
        script += ['echo "panic_details_pointer"', f"sysbus ReadDoubleWord {symbols['g_panic_abort_details']:#x}"]
    script.append("quit")
    return script, includes


def test(args, report):
    output, manifest, symbols = build_artifacts(args, report, report["scenario"])
    report.update(cpu_model="esp32c3", boot="rom-init-elf-bootstrap", sets_pc_sp=True,
                  sets_pc=True, sets_sp=False, flash_boot_validated=False)
    if args.build_only:
        report.update(status="built", runtime_status="not_run")
        return
    models, rom, dependency = dependencies(args)
    renode = shutil.which(str(Path(args.renode or os.environ.get("RENODE", "renode")).expanduser()))
    if not renode:
        raise ValueError("Renode required; pass --renode or set RENODE")
    (output / "uart.log").unlink(missing_ok=True)
    with renode_workspace(output) as (alias, temporary):
        temporary = Path(temporary)
        (temporary / "models").symlink_to(models, target_is_directory=True)
        (temporary / "rom.elf").symlink_to(rom)
        script, includes = bootstrap_script(models, alias, temporary, manifest, symbols)
        (output / "run.resc").write_text("\n".join(script) + "\n")
        bundle = output / "dotnet-bundle"
        bundle.mkdir(exist_ok=True)
        environment = dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"),
                           DOTNET_BUNDLE_EXTRACT_BASE_DIR=str(bundle), TMPDIR=str(temporary))
        report["models"] = {**dependency, "rom_sha256": sha256(rom),
            "sources_sha256": {p: sha256(models / p) for p in includes},
            "bootstrap_hooks": ["rom-main-redirect", "cycle-counter-csr", "clic-trap-dispatch", "wfi-as-nop"]}
        status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e", f"include @{alias}/run.resc"],
                          cwd=ROOT, env=environment, timeout=args.timeout + 120)
        (output / "renode.log").write_text(log)
    version = re.search(r"Renode, version (\S+)", log)
    report["renode_version"] = version[1] if version else "unknown"
    report["runtime_status"] = "failed"
    if status:
        raise ValueError("ESP32-C3 Renode execution failed; see renode.log")
    names = ("esp32c3_test_done", "esp32c3_test_checks", "esp32c3_test_failures", "esp32c3_test_last_failed",
             "emela_board_failures", "emela_board_completed", "emela_board_boot_initialized", "gpio_output", "gpio_enable")
    observations = {name: value(log, name) for name in names}
    if "g_panic_abort_details" in symbols:
        observations["panic_details_pointer"] = value(log, "panic_details_pointer")
    report["observations"] = observations
    uart = (output / "uart.log").read_text(errors="replace")
    if (observations["esp32c3_test_done"] != 1 or observations["esp32c3_test_checks"] != 3
            or observations["esp32c3_test_failures"] or observations["emela_board_failures"]
            or observations["emela_board_completed"] != 1 or observations["emela_board_boot_initialized"] != 1
            or not observations["gpio_output"] & (1 << 7) or not observations["gpio_enable"] & (1 << 7)
            or observations.get("panic_details_pointer", 0) or "Illegal instruction at PC" in log or "[ERROR]" in log
            or "EMELINO\n" not in uart):
        raise ValueError("ESP32-C3 smoke observations failed: " + str(observations))
    report.update(status="passed", runtime_status="passed",
                  model_warnings=[line for line in log.splitlines() if "[WARNING]" in line],
                  untested=["raw-flash-boot", "external-gpio-input-and-irq", "uart-rx-and-irq", "spi-peer", "i2c-peer"])
