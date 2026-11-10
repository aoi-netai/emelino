#!/usr/bin/env python3
"""Build and execute a plain Pico SDK control image, without emelino or its BSP."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests/embedded"))
from firmware.build.arm_gcc import compiler_path
from support.common import load_module, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--board", required=True, choices=["raspberry-pi-pico", "raspberry-pi-pico-2"])
    parser.add_argument("--sdk-dir", type=Path, default=os.environ.get("PICO_SDK_PATH"))
    parser.add_argument("--cc")
    parser.add_argument("--picotool-dir", type=Path, default=os.environ.get("picotool_DIR")
                        or os.environ.get("PICOTOOL_DIR") or ROOT / ".local/picotool-2.3.1/picotool")
    parser.add_argument("--rp2350js-dir", type=Path)
    parser.add_argument("--node", default="node")
    parser.add_argument("--optimization", choices=["Os", "O0"], default="Os")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    output = (args.output_dir or ROOT / "out/pico-baseline" / args.board / args.optimization).resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"schema_version": 1, "board": args.board, "build_status": "failed", "runtime_status": "not_run"}
    try:
        # Only these published files and our dedicated build tree belong to this
        # runner. Do not reuse an earlier image if configure/build fails.
        for name in ("pico_sdk_baseline.elf", "pico_sdk_baseline.uf2", "pico_sdk_baseline.bin",
                     "pico_sdk_baseline.elf.map", "pico_sdk_baseline.hex", "pico_sdk_baseline.dis",
                     "runtime.json", "report.json", "runner.json", "rp2350js.log",
                     "build-0.log", "build-1.log"):
            (output / name).unlink(missing_ok=True)
        if not args.sdk_dir:
            raise ValueError("set PICO_SDK_PATH or pass --sdk-dir")
        sdk = args.sdk_dir.resolve()
        if not (sdk / "external/pico_sdk_import.cmake").is_file():
            raise ValueError("Pico SDK not found")
        if not (args.picotool_dir / "picotoolConfig.cmake").is_file():
            raise ValueError("picotool is required; run tools/setup/pico-sdk.sh")
        cc = compiler_path(args.cc)
        target = "pico" if args.board == "raspberry-pi-pico" else "pico2"
        platform = "rp2040" if target == "pico" else "rp2350-arm-s"
        cpu = "rp2040" if target == "pico" else "rp2350-arm"
        tree = output / "sdk-build"
        if tree.is_symlink() or sdk == tree or tree in sdk.parents:
            raise ValueError("baseline SDK build directory must not alias SDK inputs")
        if tree.exists():
            shutil.rmtree(tree)
        commands = [["cmake", "-S", ROOT / "tests/rp2350js/baseline", "-B", tree,
                     "-DPICO_SDK_PATH=" + str(sdk), "-DPICO_BOARD=" + target, "-DPICO_PLATFORM=" + platform,
                     "-DPICO_TOOLCHAIN_PATH=" + str(cc.parent.parent),
                     "-Dpicotool_DIR=" + str(args.picotool_dir.resolve()), "-DCMAKE_BUILD_TYPE=MinSizeRel",
                     "-DCMAKE_C_FLAGS_MINSIZEREL=-" + args.optimization + " -DNDEBUG",
                     "-DCMAKE_CXX_FLAGS_MINSIZEREL=-" + args.optimization + " -DNDEBUG"],
                    ["cmake", "--build", tree, "--parallel", "4"]]
        for index, command in enumerate(commands):
            status, log = run(command, timeout=180)
            (output / f"build-{index}.log").write_text(log)
            if status:
                raise ValueError("baseline build failed:\n" + log)
            if index == 0:
                # The SDK caches its compiler independently of TOOLCHAIN_PATH.
                # Validate the actual choice as well as using a fresh cache.
                cache = (tree / "CMakeCache.txt").read_text().splitlines()
                selected = next((line.partition("=")[2] for line in cache
                                 if line.startswith("CMAKE_C_COMPILER:") and "=" in line), None)
                if selected is None or Path(selected).resolve() != cc:
                    raise ValueError(f"baseline SDK selected {selected}, but --cc/ARM_GCC requested {cc}")
        for name in ("pico_sdk_baseline.elf", "pico_sdk_baseline.uf2"):
            shutil.copyfile(tree / name, output / name)
        for name in ("pico_sdk_baseline.bin", "pico_sdk_baseline.elf.map",
                     "pico_sdk_baseline.hex", "pico_sdk_baseline.dis"):
            if (tree / name).is_file():
                shutil.copyfile(tree / name, output / name)
        status, nm = run([str(cc).removesuffix("gcc") + "nm", "--defined-only", output / "pico_sdk_baseline.elf"])
        if status:
            raise ValueError("baseline symbols unavailable")
        symbols = {fields[2]: int(fields[0], 16) for line in nm.splitlines() if len(fields := line.split()) == 3}
        report.update(build_status="passed", cpu_model=cpu, optimization=args.optimization)
        for name, command in (("compiler", [cc, "--version"]), ("sdk_revision", ["git", "-C", sdk, "rev-parse", "HEAD"]),
                              ("node", [args.node, "--version"])):
            status, value = run(command)
            if status:
                raise ValueError("could not determine " + name)
            report[name] = value.splitlines()[0]
        adapter = load_module(ROOT / "tests/embedded/ports/pico_sdk/rp2350js.py", "pico_baseline_adapter")
        engine, dependency = adapter.engine_directory(args)
        firmware = output / "pico_sdk_baseline.uf2"
        config = {"engine_dir": str(engine), "cpu_model": cpu, "firmware": str(firmware), "symbols": symbols,
                  "max_instructions": 10000000, "timeout_ms": 30000, "done_symbol": "pico_baseline_done",
                  "report": str(output / "runtime.json"),
                  "observe": ["pico_baseline_done", "pico_baseline_stage", "pico_baseline_elapsed", "pico_baseline_callbacks",
                              "pico_baseline_uart_rsr", "pico_baseline_i2c_enable", "pico_baseline_i2c_enable_status"]}
        (output / "runner.json").write_text(json.dumps(config, indent=2) + "\n")
        status, log = run([args.node, ROOT / "tests/rp2350js/run.cjs", output / "runner.json"], timeout=35)
        (output / "rp2350js.log").write_text(log)
        report.update(engine=dependency, runtime_status="failed")
        if not (output / "runtime.json").is_file():
            raise ValueError("baseline runtime report missing:\n" + log)
        result = json.loads((output / "runtime.json").read_text())
        report["runtime"] = result
        report["firmware_uf2_sha256"] = hashlib.sha256(firmware.read_bytes()).hexdigest()
        elapsed = result["observations"]["pico_baseline_elapsed"]
        if (status or result["reason"] != "completed" or result["uart"] != "SDK\n"
                or type(elapsed) is not int or elapsed < 100):
            raise ValueError("plain SDK control image did not pass")
        report["checks"] = {
            "boot_uart_tx_timer": True,
            "gpio_output": result["gpio25"]["output"] and result["gpio25"]["enabled"],
            "gpio_irq": result["observations"]["pico_baseline_callbacks"] == 1,
            "uart_error_status": result["observations"]["pico_baseline_uart_rsr"] == 0,
            "i2c_disable_status": result["observations"]["pico_baseline_i2c_enable"] == 0 and result["observations"]["pico_baseline_i2c_enable_status"] == 0,
        }
        report.update(runtime_status="passed" if all(report["checks"].values()) else "failed")
        if report["runtime_status"] == "failed":
            limitations = load_module(ROOT / "tests/rp2350js/limitations.py", "pico_baseline_limits")
            evidence = limitations.evidence_for(args.board, dependency["revision"], report["sdk_revision"], dependency["dist_sha256"])
            if evidence:
                baseline = evidence["baselines"][args.board]
                # O0 may observe the 100-us wait one tick later than Os. Its
                # minimum duration was checked above; all other observations
                # must still exactly match the diagnosed failure signature.
                actual = {key: value for key, value in result["observations"].items()
                          if key != "pico_baseline_elapsed"}
                expected = {key: value for key, value in baseline["observations"].items()
                            if key != "pico_baseline_elapsed"}
                if (report["checks"] == baseline["checks"] and actual == expected
                        and result["gpio25"] == baseline["gpio25"]):
                    report.update(runtime_status="model_limited", model_limitations={key: evidence["limitations"][key]
                        for key, passed in report["checks"].items() if not passed})
    except (OSError, ValueError, KeyError) as error:
        report["error"] = str(error)
        print(str(error), file=sys.stderr)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{args.board}: build={report['build_status']}, runtime={report['runtime_status']}")
    return 0 if report["runtime_status"] in {"passed", "model_limited"} else 1


if __name__ == "__main__":
    sys.exit(main())
