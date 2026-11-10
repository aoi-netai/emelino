"""Host utilities shared by firmware scenarios and standalone model checks."""
from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
from firmware.build.profiles import identifier, load_profiles

ROOT = Path(__file__).resolve().parents[3]
TESTS = ROOT / "tests/embedded"
MODELS = ROOT / "tests/renode/models"


def run(command, timeout=60, **kwargs):
    process = subprocess.Popen(list(map(str, command)), stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, errors="replace",
                               start_new_session=True, **kwargs)
    try:
        log, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        log, _ = process.communicate()
        raise ValueError("command timed out: " + str(command[0]) + "\n" + log)
    return process.returncode, log


def value(log, label):
    match = re.search(r"^" + re.escape(label) + r"\s*\n\s*(0x[0-9a-fA-F]+)\s*$", log, re.M)
    if not match:
        raise ValueError("missing Renode metric: " + label)
    return int(match[1], 16)


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_target(board, path=None):
    board = identifier(board, "board name")
    path = path or TESTS / "targets" / (board + ".json")
    target = json.loads(Path(path).read_text())
    if not isinstance(target, dict) or target.get("schema_version") != 1 or target.get("board") != board:
        raise ValueError("test target does not match the selected board")
    profile, chip, _ = load_profiles(board)
    port = profile["port"]
    simulator = target.get("simulator", "renode")
    if simulator not in {"renode", "rp2350js"}:
        raise ValueError("unsupported test simulator: " + str(simulator))
    if target.get("firmware_port", target.get("renode_port")) != port:
        raise ValueError("test target and firmware port disagree")
    if (simulator == "rp2350js") != (port == "pico_sdk"):
        raise ValueError("test simulator and firmware port disagree")
    expected_cpu = {"raspberry-pi-pico": "rp2040", "raspberry-pi-pico-2": "rp2350-arm"}
    if simulator == "rp2350js" and target.get("cpu_model") != expected_cpu.get(board):
        raise ValueError("test CPU model does not match the selected board")
    target.update(profile=profile, chip=chip, port=port, simulator=simulator)
    if not isinstance(target.get("scenarios"), dict) or not target["scenarios"]:
        raise ValueError("test target must select at least one scenario")
    for name, settings in target["scenarios"].items():
        identifier(name, "scenario name")
        if not isinstance(settings, dict):
            raise ValueError("invalid scenario settings: " + name)
        if settings.get("port", port) != port:
            raise ValueError("scenario does not support this firmware port: " + name)
        if simulator == "rp2350js":
            # These fixtures and their Node peers use the initial fixed Pico
            # wiring. Do not label a run with bindings the firmware ignores.
            if settings.keys() - {"port", "bindings", "checks", "uart", "spi", "i2c"}:
                raise ValueError("Pico scenarios do not support alternate wiring or probe settings")
            if settings.get("bindings") is not True:
                raise ValueError("Pico scenarios require their fixed fixture bindings")
            for bus, fixed in (("uart", 0), ("spi", 1), ("i2c", 1)):
                value_ = settings.get(bus, fixed)
                if type(value_) is not int or value_ != fixed:
                    raise ValueError("Pico scenarios require fixed uart0/spi1/i2c1 bindings")
    return target


def target_source(settings):
    """Generate a separate configuration module; the scenario source is unchanged."""
    imports, lines = [], []
    for kind, type_name in (("i2c", "I2c"), ("spi", "Spi"), ("uart", "Uart")):
        if kind not in settings:
            continue
        bus = settings[kind]
        lower, upper = {"uart": (0, 7), "spi": (1, 6), "i2c": (1, 3)}[kind]
        if type(bus) is not int or not lower <= bus <= upper:
            raise ValueError("invalid test bus ID")
        imports.append(f"import Embedded.{type_name}")
        lines.append(f"pub fn {kind}() -> {type_name}.{type_name}Id {{ {type_name}.{type_name}{bus} }}")
    if "cs_pin" in settings:
        pin = settings["cs_pin"]
        if type(pin) is not int or not 0 <= pin <= 65535:
            raise ValueError("invalid test chip-select pin")
        imports.append("import Embedded.Gpio")
        lines.append(f"pub fn chip_select() -> Gpio.GpioPin {{ Gpio.GpioPin({pin}) }}")
    pins = settings.get("pins", {})
    if not isinstance(pins, dict):
        raise ValueError("invalid test pin bindings")
    if pins and "import Embedded.Gpio" not in imports:
        imports.append("import Embedded.Gpio")
    for name, pin in pins.items():
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name) or type(pin) is not int or not 0 <= pin <= 65535:
            raise ValueError("invalid test pin binding")
        if name in {"chip_select", "spi", "i2c", "uart", "address", "missing_address"}:
            raise ValueError("reserved test binding name")
        lines.append(f"pub fn {name}() -> Gpio.GpioPin {{ Gpio.GpioPin({pin}) }}")
    for key in ("address", "missing_address"):
        if key in settings:
            address = settings[key]
            if type(address) is not int or not 0 <= address <= 127:
                raise ValueError("invalid I2C test address")
            lines += [f"pub fn {key}() -> Int {{ {address} }}"]
    return "\n".join(imports + lines) + "\n"


def build_artifacts(args, report, scenario):
    """Build independently of the selected simulator and retain build evidence."""
    report.update(build_status="failed", runtime_status="not_run")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("model.log", "renode.log", "uart.log", "report.json"):
        (output / name).unlink(missing_ok=True)
    fixture = TESTS / "scenarios" / scenario
    settings = args.target["scenarios"][scenario]
    source, config = fixture / "main.emel", fixture / "config.json"
    config_data = json.loads(config.read_text())
    if settings.get("bindings"):
        inputs = output / "inputs"
        inputs.mkdir(exist_ok=True)
        shutil.copyfile(source, inputs / "main.emel")
        (inputs / "test").mkdir(exist_ok=True)
        (inputs / "test/target.emel").write_text(target_source(settings))
        config_data["sources"] = ["test/target.emel"]
        source, config = inputs / "main.emel", inputs / "config.json"
        config.write_text(json.dumps(config_data, indent=2) + "\n")
    generated, firmware = output / "generated", output / "firmware"
    commands = [[args.emelino.resolve(), "build", source, "--config", config, "--out-dir", generated],
                [sys.executable, ROOT / "tools/build_firmware.py", "--board", args.board,
                 "--generated-dir", generated, "--output-dir", firmware,
                 "--optimization", args.optimization]]
    probe = fixture / "probe.c" if settings.get("bindings") else TESTS / "ports" / args.target["port"] / (scenario + "_probe.c")
    if probe.is_file():
        commands[1] += ["--source", probe]
    elif settings.get("probe", True):
        raise ValueError("missing scenario probe: " + str(probe))
    for symbol in settings.get("wrap", []):
        commands[1].append("--link-arg=-Wl,--wrap=" + symbol)
    for flag, value_ in (("--cc", args.cc), ("--sdk-dir", args.sdk_dir)):
        if value_:
            commands[1] += [flag, value_]
    for index, command in enumerate(commands):
        status, log = run(command, cwd=ROOT, timeout=120)
        (output / f"build-{index}.log").write_text(log)
        if status:
            raise ValueError("firmware generation failed:\n" + log)
        if index == 0:
            # C probes use the same logical bus selection as their Emela fixture.
            ids = {"UART": settings.get("uart", 0), "SPI": settings.get("spi", 1) - 1,
                   "I2C": settings.get("i2c", 1) - 1}
            (generated / "test_target.h").write_text("\n".join(
                f"#define EMELINO_TEST_{kind}_ID {number}" for kind, number in ids.items()) + "\n")
    manifest = json.loads((firmware / "firmware.json").read_text())
    symbols = json.loads((firmware / "symbols.json").read_text())
    report.update({"board": manifest["board"], "port": args.target["port"],
                   "resources": manifest["resources"], "firmware_sha256": manifest["sha256"]["firmware.bin"],
                   "compiler": manifest["compiler"], "sdk": manifest["sdk"],
                   "test_settings": settings})
    report["build_status"] = "passed"
    return output, manifest, symbols


def build_firmware(args, report, scenario):
    """Compatibility entry for existing Renode scenarios."""
    output, manifest, symbols = build_artifacts(args, report, scenario)
    found = shutil.which(str(Path(args.renode or os.environ.get("RENODE") or "renode").expanduser()))
    if not found:
        raise ValueError("Renode not found; pass --renode or set RENODE")
    return output, Path(found).resolve(), manifest, symbols


@contextmanager
def renode_workspace(output):
    # Renode @paths have no shell quoting. Alias also handles paths with spaces.
    with tempfile.TemporaryDirectory(prefix="emelino-renode-", dir="/tmp") as temporary:
        alias = Path(temporary) / "run"
        alias.symlink_to(output, target_is_directory=True)
        yield alias, temporary


def run_renode(renode, output, alias, temporary, script, report, timeout):
    (output / "run.resc").write_text("\n".join(script) + "\n")
    status, log = run([renode, "--disable-xwt", "--console", "--plain", "-e", f"include @{alias}/run.resc"],
                      cwd=ROOT, env=dict(os.environ, XDG_CONFIG_HOME=str(output / "renode-config"), TMPDIR=temporary),
                      timeout=timeout)
    (output / "renode.log").write_text(log)
    version = re.search(r"Renode, version (\S+)", log)
    report["renode_version"] = version[1] if version else "unknown"
    return status, log
