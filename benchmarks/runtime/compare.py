#!/usr/bin/env python3
"""Recompile saved generated C and alternate timings before/after a compiler change."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random

import run as benchmark


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--case", nargs="+", choices=list(benchmark.CASES))
    args = parser.parse_args()
    records = {mode: json.loads(path.read_text()) for mode, path in
               [("before", args.before), ("after", args.after)]}
    before, after = records["before"], records["after"]
    for key in ["cc_version", "common_c_flags", "heap_bytes", "cpu_affinity", "lto"]:
        if before[key] != after[key]:
            raise RuntimeError(f"incompatible benchmark settings: {key}")
    os.sched_setaffinity(0, {after["cpu_affinity"]})
    cc = after.get("cc_command", ["cc"])
    if benchmark.run(cc + ["--version"]).splitlines()[0] != after["cc_version"]:
        raise RuntimeError("C compiler changed")
    old = {(item["name"], item["optimization"]): item for item in before["cases"]}
    result = {"before_revision": before["git_revision"], "after_revision": after["git_revision"],
              "cc_version": after["cc_version"], "cpu_affinity": after["cpu_affinity"],
              "samples": after["samples"], "calibration_target_ms": after["minimum_sample_ms"],
              "cases": []}
    for current in after["cases"]:
        name, opt = current["name"], current["optimization"]
        if args.case and name not in args.case:
            continue
        previous = old[(name, opt)]
        for key in ["n", "source_sha256"]:
            if current[key] != previous[key]:
                raise RuntimeError(f"benchmark input changed: {name}/{key}")
        n = current["n"]
        binaries = {}
        for mode, path, item in [("before", args.before, previous), ("after", args.after, current)]:
            directory = path.parent / "build" / name
            generated = directory / "generated/emela_embedded.c"
            if hashlib.sha256(generated.read_bytes()).hexdigest() != item["generated_c_sha256"]:
                raise RuntimeError(f"saved generated C differs: {generated}")
            binary = (directory / f"compare-{opt}").resolve()
            flags = after["common_c_flags"] + [f"-{opt}", f"-DCASE_{name}", "-I", generated.parent]
            benchmark.run(cc + flags + [generated, benchmark.HERE / "harness.c",
                                       benchmark.HERE / "reference.c", "-lm", "-o", binary])
            benchmark.run([binary, "check", n, 17, 8])
            binaries[mode] = binary
        iterations = {mode: benchmark.calibrate(binary, "generated", n, after["minimum_sample_ms"] * 1e6)
                      for mode, binary in binaries.items()}
        samples = {mode: [] for mode in binaries}
        for mode, binary in binaries.items():
            benchmark.measure(binary, "generated", n, iterations[mode])
        rng = random.Random(42)
        for _ in range(after["samples"]):
            order = list(binaries)
            rng.shuffle(order)
            for mode in order:
                sample = benchmark.measure(binaries[mode], "generated", n, iterations[mode])
                samples[mode].append(sample["elapsed_ns"] / iterations[mode])
        first, last = benchmark.stats(samples["before"]), benchmark.stats(samples["after"])
        result["cases"].append({"name": name, "optimization": opt, "n": n,
                                "before": first, "after": last, "iterations": iterations,
                                "samples_ns_per_call": samples,
                                "before_c_sha256": previous["generated_c_sha256"],
                                "after_c_sha256": current["generated_c_sha256"],
                                "speedup": first["median_ns"] / last["median_ns"]})
        print(f"{name} -{opt}: {first['median_ns']/1000:.3f} -> {last['median_ns']/1000:.3f} us, "
              f"{first['median_ns']/last['median_ns']:.2f}x", flush=True)
    output = args.after.parent / "comparison.json"
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Results: {output}")


if __name__ == "__main__":
    main()
