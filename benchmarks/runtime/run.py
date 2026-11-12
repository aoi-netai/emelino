#!/usr/bin/env python3
"""Reproducible generated-C vs handwritten-C runtime benchmark (stdlib only)."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shlex
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CASES = {
    "scalar": 10000,
    "fibonacci": 20,
    "list_pipeline": 256,
    "array_pipeline": 256,
    "array_read": 256,
    "array_update": 10000,
    "closure": 10000,
    "state": 10000,
}


def run(args, **kwargs):
    result = subprocess.run([str(a) for a in args], text=True, capture_output=True,
                            timeout=120, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{shlex.join(map(str, args))}\n{result.stdout}\n{result.stderr}")
    return result.stdout.strip()


def instrument(source):
    """Counters only in a separate, untimed executable; original C stays intact."""
    counters = "#include <stdio.h>\nstatic unsigned long long audit_alloc, audit_bytes, audit_retain, audit_release, audit_copy;\n"
    signatures = {
        "static void *emela_heap_allocate(size_t payload_size, uint32_t kind) {":
            "++audit_alloc; audit_bytes += payload_size;",
        "static inline EMELA_UNUSED void emela_heap_retain(void *value) {":
            "if (value != NULL) { ++audit_retain; }\n",
        "static void emela_heap_enqueue(void *value, emela_heap_block **pending, emela_heap_block **tail) {":
            "if (value != NULL) { ++audit_release; }\n",
    }
    has_heap = "static void *emela_heap_allocate(" in source
    for signature, counter in signatures.items():
        if has_heap and source.count(signature) != 1:
            raise RuntimeError(f"audit signature changed: {signature}")
        source = source.replace(signature, signature + "\n  " + counter)
    lines = []
    for line in source.splitlines():
        lines.append(line)
        if line.startswith("static inline EMELA_UNUSED") and "_copy(const emela_array_" in line:
            lines.append("  ++audit_copy;")
    live = """
  if (emela_heap_initialized) {
    for (unsigned char *p = emela_heap_storage; p < emela_heap_end(); ) {
      emela_heap_block *b = (emela_heap_block *)(void *)p;
      if (!b->is_free) ++live;
      p += b->size;
    }
  }
""" if has_heap else ""
    footer = r'''
void bench_audit_print(void) {
  unsigned long long live = 0;
LIVE
  printf("{\"allocations\":%llu,\"requested_bytes\":%llu,\"retains\":%llu,\"releases\":%llu,\"array_copies\":%llu,\"live_blocks_after\":%llu}\n",
      audit_alloc, audit_bytes, audit_retain, audit_release, audit_copy, live);
}
'''.replace("LIVE", live)
    return counters + "\n".join(lines) + footer


def measure(binary, mode, n, iterations):
    return json.loads(run([binary, mode, n, 17, iterations]))


def calibrate(binary, mode, n, minimum_ns):
    iterations = 1
    while True:
        sample = measure(binary, mode, n, iterations)
        if sample["elapsed_ns"] >= minimum_ns:
            return iterations
        iterations *= min(16, max(2, int(minimum_ns / max(sample["elapsed_ns"], 1) * 1.1)))


def stats(values):
    median = statistics.median(values)
    return {"median_ns": median, "min_ns": min(values), "max_ns": max(values),
            "mad_ns": statistics.median(abs(v - median) for v in values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    parser.add_argument("--opt", nargs="+", choices=["Os", "O2", "O3"], default=["Os", "O2"])
    parser.add_argument("--case", nargs="+", choices=list(CASES), default=list(CASES))
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--min-ms", type=float, default=150)
    parser.add_argument("--cpu", type=int, help="default: first available CPU")
    parser.add_argument("--heap-bytes", type=int, default=65536)
    parser.add_argument("--output", type=Path, default=ROOT / "out/runtime-benchmark/results.json")
    args = parser.parse_args()
    if args.samples < 1 or args.min_ms <= 0:
        parser.error("samples and min-ms must be positive")
    if hasattr(os, "sched_getaffinity"):
        cpu = args.cpu if args.cpu is not None else min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {cpu})
    else:
        cpu = None
    args.output = args.output.resolve()
    build = args.output.parent / "build"
    build.mkdir(parents=True, exist_ok=True)
    cc = shlex.split(args.cc)
    # Current array overflow guard compares u32 with 64-bit SIZE_MAX on hosts.
    common = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-Wno-type-limits", "-pedantic"]
    result = {
        "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "git_revision": run(["git", "rev-parse", "HEAD"], cwd=ROOT),
        "compiler_version": run([args.emelino.resolve(), "--version"]),
        "cc_command": cc,
        "cc_version": run(cc + ["--version"]).splitlines()[0],
        "platform": platform.platform(), "cpu_affinity": cpu,
        "cpu_model": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                           if line.startswith("model name")), platform.processor()),
        "samples": args.samples, "minimum_sample_ms": args.min_ms,
        "common_c_flags": common, "lto": False, "heap_bytes": args.heap_bytes,
        "cases": [],
    }
    for name in args.case:
        directory = build / name
        directory.mkdir(exist_ok=True)
        source = HERE / "cases" / f"{name}.emel"
        config = directory / "config.json"
        config.write_text(json.dumps({"heap_bytes": result["heap_bytes"],
                                     "exports": [{"function": "bench", "symbol": "bench"}]}))
        run([args.emelino.resolve(), "build", source, "--config", config,
             "--out-dir", directory / "generated"], cwd=ROOT)
        generated = directory / "generated/emela_embedded.c"
        text = generated.read_text()
        audit_source = directory / "audit.c"
        audit_source.write_text(instrument(text))
        n = CASES[name]
        for opt in args.opt:
            binary = directory / opt
            flags = common + [f"-{opt}", f"-DCASE_{name}", "-I", str(generated.parent)]
            sources = [HERE / "harness.c", HERE / "reference.c"]
            run(cc + flags + [generated, *sources, "-lm", "-o", binary])
            # Boundary sizes, signed seed extremes, and multiple runtime seeds.
            sizes = [0, 1, 2, 17, n]
            for size in dict.fromkeys(sizes):
                for seed in [17, -17, -2147483648, 2147483640]:
                    run([binary, "check", size, seed, 8])
            audit_binary = directory / f"audit-{opt}"
            run(cc + flags + ["-DBENCH_AUDIT", audit_source, *sources, "-lm", "-o", audit_binary])
            audit = json.loads(run([audit_binary, "audit", n, 17, 1]))
            if audit["live_blocks_after"]:
                raise RuntimeError(f"{name}: heap leak: {audit}")
            iterations = {mode: calibrate(binary, mode, n, args.min_ms * 1e6)
                          for mode in ["generated", "c"]}
            samples = {mode: [] for mode in iterations}
            checksums = {}
            rng = random.Random(42)
            # Warm each implementation, then alternate order to reduce drift bias.
            for mode in iterations:
                measure(binary, mode, n, iterations[mode])
            for _ in range(args.samples):
                order = list(iterations)
                rng.shuffle(order)
                for mode in order:
                    sample = measure(binary, mode, n, iterations[mode])
                    checksum = sample["checksum"]
                    if mode in checksums and checksums[mode] != checksum:
                        raise RuntimeError(f"unstable checksum: {name}/{opt}/{mode}")
                    checksums[mode] = checksum
                    samples[mode].append(sample["elapsed_ns"] / iterations[mode])
            item = {"name": name, "optimization": opt, "n": n,
                    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    "generated_c_sha256": hashlib.sha256(generated.read_bytes()).hexdigest(),
                    "iterations": iterations, "checksums": checksums,
                    "samples_ns_per_call": samples, "audit": audit,
                    "generated": stats(samples["generated"]), "c": stats(samples["c"]),
                    "executable_size": run(["size", binary]), "checks": "passed"}
            item["ratio"] = item["generated"]["median_ns"] / item["c"]["median_ns"]
            result["cases"].append(item)
            args.output.write_text(json.dumps(result, indent=2) + "\n")
            print(f"{name:16s} -{opt}: generated {item['generated']['median_ns']/1000:10.3f} us, "
                  f"C {item['c']['median_ns']/1000:10.3f} us, {item['ratio']:7.2f}x; "
                  f"alloc={audit['allocations']} copy={audit['array_copies']}", flush=True)
    print(f"Results: {args.output}")


if __name__ == "__main__":
    main()
