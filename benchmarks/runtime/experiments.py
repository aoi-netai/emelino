#!/usr/bin/env python3
"""Benchmark-specific C experiments, NOT compiler transformations or general proofs.

These edit disposable generated C with guarded replacements. They demonstrate
potential for each optimization on these pure, synchronous, Int-only cases.
Production lowering must establish escape, ownership and effect preconditions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shlex

import run as baseline


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError(f"experiment no longer matches generated C: {old[:100]}")
    return text.replace(old, new, 1)


def function_body(text, signature, transform):
    start = text.index(signature + " {") + len(signature) + 2
    depth, end = 1, start
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[:start] + transform(text[start:end - 1]) + text[end - 1:]


def stack_closure(text):
    def transform(body):
        body = replace_once(body, "emela_v_5 = emela_fn_2(emela_v_2);", """
      emela_stack_env.c0 = emela_v_2;
      emela_v_5 = (emela_function_type_57){emela_fn_3, &emela_stack_env};
""")
        if body.count("emela_heap_release((emela_v_5).env);") != 2:
            raise RuntimeError("closure release sites changed")
        body = body.replace("emela_heap_release((emela_v_5).env);", "(void)0; /* stack environment */")
        return "\n  emela_closure_env_0 emela_stack_env;\n  (void)&emela_fn_2;\n" + body
    return function_body(text, "static int32_t emela_fn_1(int32_t emela_v_0, int32_t emela_v_1)", transform)


def direct_array_map(text):
    return function_body(text, "static void * emela_fn_2(void * emela_v_0, emela_function_type_58 emela_v_1)", lambda _: """
  /* Preserve borrowed input/callback and return one owned Int array. */
  (void)&emela_fn_5; (void)&emela_fn_6;
  const emela_array_57 *input = emela_v_0;
  uint32_t n = input == NULL ? 0 : input->length;
  emela_array_57 *result = emela_array_57_new(n);
  for (uint32_t i = 0; i < n; ++i) {
    if (emela_v_1.call == NULL)
      emela_board_defect(EMELA_DEFECT_PANIC, "null function value", UINT16_C(19));
    result->values[i] = emela_v_1.call(emela_v_1.env, input->values[i]);
  }
  return result;
""")


def invariant_array_reference(text):
    def transform(body):
        body = replace_once(body, "          emela_heap_retain(emela_recur_tmp_0);", "          /* Same pointer remains owned across the loop backedge. */")
        return replace_once(body, """          if (emela_owned_v_5 != INT32_C(0)) {
            emela_heap_release(emela_v_5);
          }
          emela_owned_v_5 = INT32_C(0);""", "")
    return function_body(text, "static int32_t emela_fn_5(void * emela_v_0, int32_t emela_v_1, emela_function_type_58 emela_v_2, int32_t emela_v_3)", transform)


def borrowed_state_access(text):
    # The enclosing State.run owns the Int cell throughout both synchronous calls.
    for number in [2, 3]:
        signature = ("static int32_t emela_fn_2(void *emela_env)" if number == 2 else
                     "static void emela_fn_3(void *emela_env, int32_t emela_v_1)")
        def transform(body, number=number):
            body = replace_once(body, f"  emela_heap_retain(((emela_closure_env_{number - 2} *)emela_env)->c0);", "")
            return replace_once(body, "    emela_heap_release(emela_v_0);", "    (void)0; /* borrowed cell */")
        text = function_body(text, signature, transform)
    return text


VARIANTS = {
    "closure": ("stack_capture", stack_closure),
    "array_pipeline": ("direct_array_map", direct_array_map),
    "array_read": ("invariant_array_reference", invariant_array_reference),
    "state": ("borrowed_state_access", borrowed_state_access),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    args = parser.parse_args()
    result = json.loads(args.baseline.read_text())
    os.sched_setaffinity(0, {result["cpu_affinity"]})
    output = args.baseline.parent / "experiments.json"
    build = args.baseline.parent / "build"
    experiments = {"baseline_file": args.baseline.name, "experiments": []}
    for name, (variant, transform) in VARIANTS.items():
        if not any(item["name"] == name for item in result["cases"]):
            continue
        directory = build / name
        original = directory / "generated/emela_embedded.c"
        source = directory / f"{variant}.c"
        source.write_text(transform(original.read_text()))
        for item in result["cases"]:
            if item["name"] != name:
                continue
            if hashlib.sha256(original.read_bytes()).hexdigest() != item["generated_c_sha256"]:
                raise RuntimeError("baseline C differs; rerun baseline")
            opt, n = item["optimization"], item["n"]
            flags = result["common_c_flags"] + [f"-{opt}", f"-DCASE_{name}", "-I", str(original.parent)]
            cc = result.get("cc_command", shlex.split(os.environ.get("CC", "cc")))
            sources = [baseline.HERE / "harness.c", baseline.HERE / "reference.c"]
            binary = directory / f"{variant}-{opt}"
            baseline.run(cc + flags + [source, *sources, "-lm", "-o", binary])
            for size in [0, 1, 2, 17, n]:
                for seed in [17, -17, -2147483648, 2147483640]:
                    baseline.run([binary, "check", size, seed, 8])
            audit_source = directory / f"{variant}-audit.c"
            audit_source.write_text(baseline.instrument(source.read_text()))
            audit_binary = directory / f"{variant}-audit-{opt}"
            baseline.run(cc + flags + ["-DBENCH_AUDIT", audit_source, *sources, "-lm", "-o", audit_binary])
            audit = json.loads(baseline.run([audit_binary, "audit", n, 17, 1]))
            if audit["live_blocks_after"]:
                raise RuntimeError(f"leak in {variant}: {audit}")
            binaries = {"before": directory / opt, "after": binary}
            iterations = {mode: baseline.calibrate(path, "generated", n, result["minimum_sample_ms"] * 1e6)
                          for mode, path in binaries.items()}
            samples = {mode: [] for mode in binaries}
            for mode, path in binaries.items():
                baseline.measure(path, "generated", n, iterations[mode])
            rng = random.Random(42)
            for _ in range(result["samples"]):
                order = list(binaries)
                rng.shuffle(order)
                for mode in order:
                    sample = baseline.measure(binaries[mode], "generated", n, iterations[mode])
                    samples[mode].append(sample["elapsed_ns"] / iterations[mode])
            before, after = baseline.stats(samples["before"]), baseline.stats(samples["after"])
            exp = {"case": name, "variant": variant, "optimization": opt, "n": n,
                   "samples_ns_per_call": samples, "iterations": iterations,
                   "before": before, "after": after,
                   "speedup": before["median_ns"] / after["median_ns"],
                   "audit": audit, "checks": "passed",
                   "experiment_c_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
            experiments["experiments"].append(exp)
            output.write_text(json.dumps(experiments, indent=2) + "\n")
            print(f"{name:16s} -{opt}: {before['median_ns']/1000:.3f} -> {after['median_ns']/1000:.3f} us, "
                  f"{exp['speedup']:.2f}x; {audit}", flush=True)
    print(f"Results: {output}")


if __name__ == "__main__":
    main()
