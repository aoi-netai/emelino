"""Select a board's SDK adapter and build a generated Emelino artifact."""

import argparse
import importlib
import json
from pathlib import Path
import subprocess
import sys

from firmware.build.artifact import load_manifest
from firmware.build.profiles import load_profiles
from firmware.build.support import CommandLog, record_artifacts, sha256


def build(args):
    generated, output = args.generated_dir.resolve(), args.output_dir.resolve()
    if generated == output:
        raise ValueError("generated and firmware output directories must differ")
    output.mkdir(parents=True, exist_ok=True)
    # No failure, including profile/manifest validation, may preserve old success.
    (output / "firmware.json").unlink(missing_ok=True)
    profile, chip, board_dir = load_profiles(args.board)
    inputs = load_manifest(generated, require_aliases=True)
    metadata = json.loads((generated / "emela_artifact.json").read_text())["metadata"]
    if not metadata["entry"]:
        raise ValueError("firmware requires an entry in the Emelino configuration")
    backend = importlib.import_module("firmware.ports." + profile["port"] + ".build")
    with CommandLog(output / "build.log") as log:
        result = backend.build(args, profile, chip, board_dir, inputs, metadata, log.run)
    result.update({"board": profile["board"], "port": profile["port"],
                   "chip": chip["id"], "mcu": chip["mcu"], "optimization": args.optimization,
                   "profiles_sha256": {
                       "board": sha256(board_dir / "board.json"),
                       "chip": sha256(board_dir.parents[1] / "chips" / (chip["id"] + ".json"))}})
    return record_artifacts(output, result)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--generated-dir", required=True, type=Path)
    result.add_argument("--output-dir", required=True, type=Path)
    result.add_argument("--board", default="nucleo-f446re")
    result.add_argument("--cc", help="compiler executable; interpreted by the selected port")
    result.add_argument("--sdk-dir", type=Path, help="SDK root for the selected port")
    result.add_argument("--source", action="append", type=Path, default=[])
    result.add_argument("--link-arg", action="append", default=[], help="extra linker argument (repeatable)")
    result.add_argument("--optimization", choices=["O0", "O1", "O2", "Os"], default="Os")
    result.add_argument("--stack-bytes", type=int, default=16384)
    result.add_argument("--guard-bytes", type=int, default=256)
    return result


def main():
    try:
        build(parser().parse_args())
        return 0
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        return 1
