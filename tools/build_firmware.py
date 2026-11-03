#!/usr/bin/env python3
"""Build firmware using the board's selected SDK port."""

from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from firmware.build.cli import main


if __name__ == "__main__":
    sys.exit(main())
