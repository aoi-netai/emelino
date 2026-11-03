"""Locate board and MCU descriptions independently of any SDK."""

import json
from pathlib import Path
import re

FIRMWARE = Path(__file__).resolve().parents[1]


def identifier(value, label):
    if not isinstance(value, str) or not re.fullmatch(r"[a-z][a-z0-9_-]*", value):
        raise ValueError("invalid " + label)
    return value


def load_profiles(board, root=FIRMWARE):
    """Return board, chip and board directory; IDs never contain filesystem paths."""
    identifier(board, "board name")
    board_dir = root / "boards" / board
    profile = json.loads((board_dir / "board.json").read_text())
    if not isinstance(profile, dict) or type(profile.get("schema_version")) is not int or profile["schema_version"] != 1:
        raise ValueError("unsupported board profile schema")
    chip_id = identifier(profile.get("chip"), "chip name")
    port = identifier(profile.get("port"), "port name")
    chip = json.loads((root / "chips" / (chip_id + ".json")).read_text())
    if not isinstance(chip, dict) or type(chip.get("schema_version")) is not int or chip["schema_version"] != 1:
        raise ValueError("unsupported chip profile schema")
    if profile.get("board") != board or chip.get("id") != chip_id or chip.get("port") != port:
        raise ValueError("board, chip and port profiles disagree")
    if not (root / "ports" / port / "build.py").is_file():
        raise ValueError("firmware port is not implemented: " + port)
    return profile, chip, board_dir
