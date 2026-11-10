"""Narrow signatures backed by the fixed engine's plain-SDK control image.

Unexpected checks, faults, incomplete execution, timeouts and missing tools are
never relabelled as model limitations. Other SDK/engine versions need evidence.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def evidence_for(board, engine_revision, sdk_revision, engine_dist_sha256):
    evidence = json.loads((ROOT / "tests/rp2350js/model-limitations.json").read_text())
    if (evidence["engine_revision"] != engine_revision or evidence["sdk_revision"] != sdk_revision
            or evidence["engine_dist_sha256"] != engine_dist_sha256):
        return None
    for source, digest in {**evidence["baseline_sources"], **evidence["diagnosed_sources"]}.items():
        if hashlib.sha256((ROOT / source).read_bytes()).hexdigest() != digest:
            return None
    return evidence if board in evidence["baselines"] else None


def classify(board, scenario, result, expected_checks, engine_revision, sdk_revision, engine_dist_sha256):
    """Return measured limitations, or None for an unclassified failed run."""
    evidence = evidence_for(board, engine_revision, sdk_revision, engine_dist_sha256)
    if not evidence or result.get("reason") != "completed" or result.get("error"):
        return None
    observations = result.get("observations", {})
    if (observations.get("pico_test_done") != 1 or observations.get("emela_board_failures") != 0
            or result.get("cpu_model") != evidence["baselines"][board]["cpu_model"]):
        return None
    checks = observations.get("pico_test_checks")
    failures = result.get("failed_checks", [])
    if observations.get("pico_test_failed") != len(failures):
        return None
    m33 = board == "raspberry-pi-pico-2"
    keys, remaining = [], []
    if scenario == "pico_gpio":
        if not m33 or checks != expected_checks or failures != [2]:
            return None
        if observations.get("pico_test_callbacks") != 0 or observations.get("pico_test_isr") != 0:
            return None
        if result.get("gpio25") != {"output": False, "enabled": False, "function": 5}:
            return None
        keys = ["gpio_irq", "gpio_output"]
    elif scenario == "pico_spi":
        transfers = result.get("spi_transfers", [])
        if (result.get("spi_words") != [18, 52, 86, 120, 4660]
                or [item.get("bits") for item in transfers] != [8, 8, 8, 8, 16]):
            return None
        selected = [item for item in transfers if item["word"] in (86, 120)]
        if not m33 or checks != expected_checks or failures or len(selected) != 2:
            return None
        # Expected missing output-enable from the CP0 bug; a wrong active
        # output level or omitted transfer is not silently excused.
        if any(item["cs_output"] or item["cs_enabled"] for item in selected):
            return None
        changes = [item for item in result.get("gpio", []) if item["pin"] == 17]
        if not changes or changes[-1]["value"] != 2:
            return None
        keys = ["gpio_output"]
    elif scenario in {"pico_uart", "pico_peripherals"}:
        combined = scenario == "pico_peripherals"
        expected = ([2] if combined and m33 else []) + [9701]
        if failures != expected or checks != (5 if combined else 2) or result.get("uart") != "PICO\n":
            return None
        keys = ["uart_error_status"]
        remaining = ["UART IRQ RX/TX and timeout checks"]
        if combined:
            if result.get("gpio25") != {"output": not m33, "enabled": not m33, "function": 5}:
                return None
            remaining += ["SPI and I2C in this combined scenario (tested separately)"]
            if m33:
                if observations.get("pico_test_callbacks") != 0 or observations.get("pico_test_isr") != 0:
                    return None
                keys += ["gpio_irq", "gpio_output"]
            elif observations.get("pico_test_callbacks") != 1 or observations.get("pico_test_isr") != 1:
                return None
    elif scenario == "pico_i2c":
        if failures != [10001] or checks != 1 or result.get("i2c_events"):
            return None
        keys = ["i2c_disable_status"]
        remaining = ["I2C transfers, Repeated START, IRQ, recovery (no bus transaction started)"]
    else:
        return None
    baseline = evidence["baselines"][board]
    if any(baseline["checks"].get(key) is not False for key in keys):
        return None
    return {"evidence": "tests/rp2350js/model-limitations.json", "baseline": baseline,
            "limitations": [{"feature": key, **evidence["limitations"][key]} for key in keys],
            "not_exercised": remaining}
