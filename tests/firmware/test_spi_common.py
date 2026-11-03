#!/usr/bin/env python3
"""Test common SPI against two independent host buses and generated ABI headers."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emelino", type=Path, default=ROOT / "target/debug/emelino")
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="emelino-common-spi-") as directory:
        work = Path(directory)
        source = work / "main.emel"
        source.write_text('''import Embedded.Board.{BoardSpi}
import Embedded.Spi
pub fn main() -> () {
  with BoardSpi {
    (use Spi.Spi).configure(Spi.Spi1, Spi.SpiConfig(Spi.Master, Spi.TwoLine, Spi.Bits8,
      Spi.IdleLow, Spi.FirstEdge, 1000000, Spi.MostSignificantFirst))
  } escape { Spi.SpiError(_) -> () }
}
''')
        config = work / "config.json"
        config.write_text(json.dumps({"entry": "main", "heap_bytes": 16384,
            "exports": [{"function": "main", "symbol": "test_main"}]}))
        generated = work / "generated"
        subprocess.run([str(args.emelino), "build", str(source), "--config", str(config),
                        "--out-dir", str(generated)], cwd=ROOT, check=True)
        for optimization in ("O0", "Os"):
            binary = work / ("test-" + optimization)
            subprocess.run([args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror",
                            "-fsanitize=undefined", "-fno-sanitize-recover=all", "-" + optimization,
                            "-I" + str(generated), "-I" + str(ROOT / "firmware/common"),
                            str(ROOT / "firmware/common/spi.c"), str(Path(__file__).with_name("spi_common.c")),
                            "-o", str(binary)], check=True)
            for case in ("basic", "cleanup", "deadline", "registry", "isolation", "invalid"):
                subprocess.run([str(binary), case], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
