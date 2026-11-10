"""Pico SDK artifacts executed by the fixed, free rp2350js engine."""
import hashlib
import json
import os
from pathlib import Path
import shutil

from support.common import ROOT, build_artifacts, load_module, run


class Platform:
    def __init__(self, target):
        self.target = target


def engine_directory(args):
    dependency = json.loads((ROOT / "tests/rp2350js/dependency.json").read_text())
    directory = (args.rp2350js_dir or Path(os.environ.get("RP2350JS_DIR", ROOT / dependency["directory"]))).expanduser().resolve()
    if not (directory / "dist/cjs/index.js").is_file():
        raise ValueError("rp2350js not built; run tools/setup/rp2350js.sh or pass --rp2350js-dir")
    status, revision = run(["git", "-C", directory, "rev-parse", "HEAD"])
    if status or revision.strip() != dependency["revision"]:
        raise ValueError("rp2350js revision does not match tests/rp2350js/dependency.json")
    status, modified = run(["git", "-C", directory, "status", "--porcelain", "--untracked-files=no"])
    if status or modified.strip():
        raise ValueError("rp2350js tracked sources are modified")
    digest = hashlib.sha256()
    for file in sorted((directory / "dist/cjs").rglob("*.js")):
        digest.update(str(file.relative_to(directory / "dist/cjs")).encode() + b"\0")
        digest.update(file.read_bytes())
    dependency["dist_sha256"] = digest.hexdigest()
    return directory, dependency


def test(args, report):
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for name in ("runtime.json", "rp2350js.log", "runner.json"):
        (args.output_dir / name).unlink(missing_ok=True)
    output, manifest, symbols = build_artifacts(args, report, report["scenario"])
    report.update(cpu_model=args.target["cpu_model"], boot="bundled-bootrom-flash-reset")
    if args.build_only:
        report.update(status="built", runtime_status="not_run")
        return
    directory, dependency = engine_directory(args)
    node = shutil.which(args.node)
    if not node:
        raise ValueError("Node.js not found: " + args.node)
    status, version = run([node, "--version"])
    if status:
        raise ValueError("cannot determine Node.js version")
    try:
        major = int(version.strip().lstrip("v").split(".")[0])
    except ValueError:
        raise ValueError("invalid Node.js version: " + version.strip()) from None
    if major < dependency["node_minimum"]:
        raise ValueError("Node.js 18 or later is required")
    report["engine"] = {**dependency, "node": version.strip()}
    firmware = output / "firmware/firmware.uf2"
    config = {
        "engine_dir": str(directory), "cpu_model": args.target["cpu_model"],
        "firmware": str(firmware), "symbols": symbols,
        "max_instructions": args.max_instructions, "timeout_ms": int(args.timeout * 1000),
        "done_symbol": "pico_test_done", "report": str(output / "runtime.json"),
        "observe": ["pico_test_done", "pico_test_checks", "pico_test_failed", "pico_test_last_failed",
                    "pico_test_callbacks", "pico_test_isr", "emela_board_failures", "emela_board_last_defect"],
    }
    config_path = output / "runner.json"
    config_path.write_text(json.dumps(config, indent=2) + "\n")
    status, log = run([node, ROOT / "tests/rp2350js/run.cjs", config_path], timeout=args.timeout + 5)
    (output / "rp2350js.log").write_text(log)
    if not (output / "runtime.json").is_file():
        raise ValueError("rp2350js did not produce a runtime report:\n" + log)
    result = json.loads((output / "runtime.json").read_text())
    uf2_digest = hashlib.sha256(firmware.read_bytes()).hexdigest()
    if (result.get("firmware_sha256") != uf2_digest or result.get("cpu_model") != args.target["cpu_model"]
            or result.get("boot") != "bundled-bootrom-flash-reset"
            or result.get("sets_pc_sp") is not False or result.get("sets_vtor") is not False):
        raise ValueError("runtime report does not match the selected firmware, CPU model or boot method")
    report.update(runtime=result, runtime_status="failed", logs=["rp2350js.log", "runtime.json"],
                  firmware_uf2_sha256=uf2_digest)
    if status or result["reason"] != "completed":
        raise ValueError("rp2350js execution did not complete: " + result["reason"])
    observations = result["observations"]
    expected = args.target["scenarios"][report["scenario"]]["checks"]
    if (observations.get("pico_test_done") != 1 or observations.get("emela_board_failures") != 0
            or result.get("error") is not None):
        raise ValueError("firmware defect or invalid completion observation")
    success = not observations["pico_test_failed"] and observations["pico_test_checks"] == expected
    if report["scenario"] in {"pico_gpio", "pico_peripherals"}:
        success = success and observations["pico_test_callbacks"] == 1 and observations["pico_test_isr"] == 1
        success = success and result["gpio25"] == {"output": True, "enabled": True, "function": 5}
    if report["scenario"] in {"pico_spi", "pico_peripherals"}:
        selected = [item for item in result["spi_transfers"] if item["word"] in (86, 120)]
        success = success and len(selected) == 2 and all(item["cs_enabled"] and not item["cs_output"] for item in selected)
        success = success and result["spi_words"] == [18, 52, 86, 120, 4660]
        success = success and [item["bits"] for item in result["spi_transfers"]] == [8, 8, 8, 8, 16]
        changes = [item for item in result["gpio"] if item["pin"] == 17]
        success = success and bool(changes) and changes[-1]["value"] == 1
    if not success:
        limitations = load_module(ROOT / "tests/rp2350js/limitations.py", "pico_model_limitations")
        evidence = limitations.classify(args.board, report["scenario"], result, expected,
                                       dependency["revision"], manifest["sdk"]["revision"], dependency["dist_sha256"])
        if evidence:
            report.update(status="model_limited", runtime_status="model_limited", model_limitations=evidence)
            return
        raise ValueError("firmware checks or physical observations failed: " + str(observations))
    report.update(status="passed", runtime_status="passed")
