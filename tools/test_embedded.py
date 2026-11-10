#!/usr/bin/env python3
"""Build and run shared embedded scenarios for a selected board."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests/embedded"))
from support.common import TESTS, load_module, load_target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--board", default="nucleo-f446re")
    parser.add_argument("--target", type=Path, help="alternate test wiring for the selected board")
    parser.add_argument("--scenario", action="append", help="scenario name (repeatable), or all; defaults to all")
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--renode")
    parser.add_argument("--rp2350js-dir", type=Path, help="fixed rp2350js checkout (or RP2350JS_DIR)")
    parser.add_argument("--node", default="node")
    parser.add_argument("--build-only", action="store_true", help="Pico: build SDK artifacts without claiming a runtime result")
    parser.add_argument("--max-instructions", type=int, default=10000000)
    parser.add_argument("--cc")
    parser.add_argument("--sdk-dir", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    parser.add_argument("--timeout", type=float, default=60)
    controller = parser.add_mutually_exclusive_group()
    controller.add_argument("--controller-model", type=Path, help="override the BMP180 target's I2C controller model")
    controller.add_argument("--stock-controller", action="store_true", help="measure BMP180 with Renode's bundled I2C controller")
    parser.add_argument("--baseline-report", type=Path, help="require byte-identical BMP180 firmware to a previous run")
    args = parser.parse_args()
    try:
        args.target = load_target(args.board, args.target)
        names = args.scenario or ["all"]
        if "all" in names:
            if names != ["all"]:
                raise ValueError("all cannot be combined with individual scenarios")
            names = list(args.target["scenarios"])
        if len(set(names)) != len(names) or any(name not in args.target["scenarios"] for name in names):
            raise ValueError("unknown or duplicate scenario; available: " + ", ".join(args.target["scenarios"]))
        if args.timeout <= 0 or args.max_instructions <= 0:
            raise ValueError("timeout and instruction limit must be positive")
        simulator = args.target["simulator"]
        if args.build_only and simulator != "rp2350js":
            raise ValueError("--build-only is currently supported for Pico targets")
        adapter = load_module(TESTS / "ports" / args.target["port"] / (simulator + ".py"), "embedded_simulator_port")
        args.platform = adapter.Platform(args.target)
        base = (args.output_dir or ROOT / "out/embedded" / args.board / args.optimization).resolve()
        passed = True
        summaries = []
        for name in names:
            current = copy.copy(args)
            current.output_dir = base / name if len(names) > 1 or args.output_dir is None else base
            settings = args.target["scenarios"][name]
            if name == "bmp180" and current.controller_model is None and not args.stock_controller:
                model = settings.get("controller_model")
                current.controller_model = ROOT / model if model else None
            report = {"schema_version": 1, "status": "running", "scenario": name,
                      "board": args.board, "simulator": simulator,
                      "build_status": "not_run", "runtime_status": "not_run",
                      "optimization": args.optimization, "boot": "raw-flash-reset",
                      "sets_pc_sp": False, "sets_vtor": False}
            try:
                if simulator == "rp2350js":
                    adapter.test(current, report)
                else:
                    location = TESTS / "scenarios" / name / "test.py"
                    if settings.get("port"):
                        location = TESTS / "ports" / settings["port"] / (name + ".py")
                    scenario = load_module(location, "embedded_scenario_" + name)
                    scenario.test(current, report)
                    if report["status"] == "passed":
                        report["runtime_status"] = "passed"
                if report["status"] not in {"passed", "model_limited", "built"}:
                    raise ValueError("scenario did not report success")
                print(f"{report['status'].upper()}: {args.board}/{name} ({args.optimization})")
            except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
                report.update(status="failed", error=str(error))
                if report["build_status"] == "passed":
                    report["runtime_status"] = "failed"
                print(f"FAIL: {args.board}/{name}: {error}", file=sys.stderr)
                passed = False
            current.output_dir.mkdir(parents=True, exist_ok=True)
            (current.output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            summaries.append({"scenario": name, "status": report["status"],
                              "build_status": report["build_status"], "runtime_status": report["runtime_status"],
                              "report": str(current.output_dir / "report.json")})
        if len(names) > 1:
            statuses = {case["status"] for case in summaries}
            overall = ("failed" if not passed else "model_limited" if "model_limited" in statuses
                       else "built" if statuses == {"built"} else "passed")
            runtime_states = {case["runtime_status"] for case in summaries}
            runtime_status = ("failed" if "failed" in runtime_states
                              else "not_run" if "not_run" in runtime_states
                              else "model_limited" if "model_limited" in runtime_states else "passed")
            (base / "report.json").write_text(json.dumps({"schema_version": 1, "board": args.board,
                "optimization": args.optimization, "status": overall,
                "build_status": "passed" if all(case["build_status"] == "passed" for case in summaries) else "failed",
                "runtime_status": runtime_status, "scenarios": summaries}, indent=2) + "\n")
        return 0 if passed else 1
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    sys.exit(main())
