#!/usr/bin/env python3
"""Run ASan/UBSan correctness checks on an existing runtime benchmark build."""
import argparse
import json
import os

import run as baseline
from experiments import VARIANTS


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=baseline.Path, required=True)
    args = parser.parse_args()
    result = json.loads(args.baseline.read_text())
    flags = ["-std=c11", "-O1", "-g", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
    records = []
    for name in dict.fromkeys(item["name"] for item in result["cases"]):
        n = next(item["n"] for item in result["cases"] if item["name"] == name)
        directory = args.baseline.parent / "build" / name
        sources = [("baseline", directory / "generated/emela_embedded.c")]
        if name in VARIANTS:
            variant = VARIANTS[name][0]
            source = directory / f"{variant}.c"
            if source.exists():
                sources.append((variant, source))
        for variant, source in sources:
            binary = (directory / f"sanitize-{variant}").resolve()
            baseline.run(result.get("cc_command", ["cc"]) + flags + [f"-DCASE_{name}", "-I", directory / "generated",
                         source, baseline.HERE / "harness.c", baseline.HERE / "reference.c", "-lm", "-o", binary])
            for size in [0, 17, n]:
                for seed in [-2147483648, 2147483640]:
                    baseline.run([binary, "check", size, seed, 8])
            records.append({"case": name, "variant": variant, "sizes": [0, 17, n],
                            "seeds": [-2147483648, 2147483640], "repetitions": 8, "status": "passed"})
            print(f"{name} {variant}: ASan/UBSan passed", flush=True)
    output = args.baseline.parent / "sanitizers.json"
    output.write_text(json.dumps({"flags": flags, "ASAN_OPTIONS": os.environ.get("ASAN_OPTIONS", ""),
                                  "checks": records}, indent=2) + "\n")


if __name__ == "__main__":
    main()
