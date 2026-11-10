"""Reject stale/mismatched runtime evidence and unrelated emulator failures."""
import hashlib
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests/embedded"))


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


limits = load("tests/rp2350js/limitations.py", "pico_limits_test")
adapter = load("tests/embedded/ports/pico_sdk/rp2350js.py", "pico_adapter_test")


def result(scenario, m33=False):
    observations = {"pico_test_done": 1, "pico_test_checks": 4, "pico_test_failed": 0,
        "pico_test_last_failed": 0, "pico_test_callbacks": 0, "pico_test_isr": 0,
        "emela_board_failures": 0, "emela_board_last_defect": 0}
    words = [18, 52, 86, 120, 4660] if scenario == "pico_spi" else []
    data = {"reason": "completed", "error": None, "observations": observations,
        "failed_checks": [], "uart": "", "i2c_events": [], "spi_words": words,
        "spi_transfers": [{"word": word, "bits": 16 if word == 4660 else 8,
            "cs_output": not (word in (86, 120)) if not m33 else False,
            "cs_enabled": not m33} for word in words],
        "gpio25": {"output": not m33, "enabled": not m33, "function": 5},
        "gpio": [{"pin": 17, "value": 2 if m33 else 1, "previous": 0}] if scenario == "pico_spi" else [],
        "cpu_model": "rp2350-arm" if m33 else "rp2040",
        "boot": "bundled-bootrom-flash-reset", "sets_pc_sp": False, "sets_vtor": False}
    if scenario in {"pico_uart", "pico_peripherals"}:
        data["uart"] = "PICO\n"
        data["failed_checks"] = ([2] if m33 and scenario == "pico_peripherals" else []) + [9701]
        observations["pico_test_checks"] = 5 if scenario == "pico_peripherals" else 2
        observations["pico_test_last_failed"] = 9701
        if scenario == "pico_peripherals" and not m33:
            observations["pico_test_callbacks"] = observations["pico_test_isr"] = 1
    elif scenario == "pico_i2c":
        data["failed_checks"] = [10001]
        observations["pico_test_checks"] = 1
        observations["pico_test_last_failed"] = 10001
    elif scenario == "pico_gpio":
        observations["pico_test_checks"] = 3
        data["failed_checks"] = [2] if m33 else []
        observations["pico_test_callbacks"] = observations["pico_test_isr"] = 0 if m33 else 1
        observations["pico_test_last_failed"] = 2 if m33 else 0
    observations["pico_test_failed"] = len(data["failed_checks"])
    return data


class ClassificationTests(unittest.TestCase):
    def setUp(self):
        self.evidence = json.loads((ROOT / "tests/rp2350js/model-limitations.json").read_text())

    def classify(self, scenario, data, m33=False, **identity):
        return limits.classify("raspberry-pi-pico-2" if m33 else "raspberry-pi-pico", scenario,
            data, {"pico_gpio": 3, "pico_uart": 4, "pico_spi": 4, "pico_i2c": 7, "pico_peripherals": 18}[scenario],
            identity.get("engine", self.evidence["engine_revision"]),
            identity.get("sdk", self.evidence["sdk_revision"]),
            identity.get("dist", self.evidence["engine_dist_sha256"]))

    def test_only_documented_board_and_failure_signatures_are_accepted(self):
        for scenario in ("pico_uart", "pico_i2c", "pico_peripherals"):
            for m33 in (False, True):
                with self.subTest(scenario=scenario, m33=m33):
                    self.assertIsNotNone(self.classify(scenario, result(scenario, m33), m33))
        for scenario in ("pico_gpio", "pico_spi"):
            self.assertIsNotNone(self.classify(scenario, result(scenario, True), True))
            self.assertIsNone(self.classify(scenario, result(scenario), False))

    def test_version_and_executed_javascript_changes_invalidate_evidence(self):
        for identity in ("engine", "sdk", "dist"):
            with self.subTest(identity=identity):
                self.assertIsNone(self.classify("pico_uart", result("pico_uart"), **{identity: "changed"}))

    def test_changed_bsp_or_plain_sdk_control_invalidates_prior_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record = root / "tests/rp2350js/model-limitations.json"
            record.parent.mkdir(parents=True)
            evidence = dict(self.evidence)
            digest = hashlib.sha256(b"diagnosed source").hexdigest()
            evidence["baseline_sources"] = {"baseline.c": digest}
            evidence["diagnosed_sources"] = {"driver.c": digest}
            record.write_text(json.dumps(evidence))
            (root / "baseline.c").write_bytes(b"diagnosed source")
            (root / "driver.c").write_bytes(b"diagnosed source")
            with patch.object(limits, "ROOT", root):
                arguments = ("raspberry-pi-pico", evidence["engine_revision"], evidence["sdk_revision"], evidence["engine_dist_sha256"])
                self.assertIsNotNone(limits.evidence_for(*arguments))
                for name in ("driver.c", "baseline.c"):
                    (root / name).write_bytes(b"changed after diagnosis")
                    self.assertIsNone(limits.evidence_for(*arguments))
                    (root / name).write_bytes(b"diagnosed source")

    def test_timeout_faults_and_extra_failures_cannot_be_relabelled(self):
        for reason in ("host_timeout", "instruction_limit", "engine_error", "firmware_defect"):
            data = result("pico_uart"); data["reason"] = reason
            with self.subTest(reason=reason):
                self.assertIsNone(self.classify("pico_uart", data))
        data = result("pico_uart"); data["error"] = "unexpected engine exception"
        self.assertIsNone(self.classify("pico_uart", data))
        data = result("pico_uart"); data["observations"]["emela_board_failures"] = 1
        self.assertIsNone(self.classify("pico_uart", data))
        data = result("pico_uart"); data["failed_checks"].append(1234)
        data["observations"]["pico_test_failed"] += 1
        self.assertIsNone(self.classify("pico_uart", data))
        data = result("pico_uart"); data["observations"]["pico_test_failed"] += 1
        self.assertIsNone(self.classify("pico_uart", data))
        data = result("pico_uart"); data["observations"]["pico_test_done"] = 0
        self.assertIsNone(self.classify("pico_uart", data))

    def test_different_error_type_or_progress_is_not_the_known_limitation(self):
        for scenario, changes in (("pico_uart", {"failed_checks": [97]}),
                                   ("pico_uart", {"uart": "PICO\nIRQ\n"}),
                                   ("pico_i2c", {"failed_checks": [100]}),
                                   ("pico_i2c", {"i2c_events": [{"start": True}]})):
            data = result(scenario); data.update(changes)
            with self.subTest(scenario=scenario, changes=changes):
                self.assertIsNone(self.classify(scenario, data))
        data = result("pico_i2c"); data["observations"]["pico_test_checks"] = 2
        self.assertIsNone(self.classify("pico_i2c", data))

    def test_wrong_cs_level_and_missing_transfers_are_not_excused(self):
        data = result("pico_spi", True); data["spi_transfers"][2]["cs_output"] = True
        self.assertIsNone(self.classify("pico_spi", data, True))
        data = result("pico_spi", True); data["spi_transfers"].pop(2)
        self.assertIsNone(self.classify("pico_spi", data, True))
        data = result("pico_spi", True); data["spi_transfers"][-1]["bits"] = 8
        self.assertIsNone(self.classify("pico_spi", data, True))
        data = result("pico_spi", True); data["gpio"][-1]["value"] = 0
        self.assertIsNone(self.classify("pico_spi", data, True))

    def test_known_uart_problem_cannot_hide_unrelated_gpio_failure(self):
        for m33 in (False, True):
            for field, wrong in (("output", m33), ("enabled", m33), ("function", 0)):
                data = result("pico_peripherals", m33)
                data["gpio25"][field] = wrong
                with self.subTest(m33=m33, field=field):
                    self.assertIsNone(self.classify("pico_peripherals", data, m33))
        data = result("pico_gpio", True); data["gpio25"]["function"] = 0
        self.assertIsNone(self.classify("pico_gpio", data, True))


class AdapterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        (self.output / "firmware").mkdir()
        firmware = self.output / "firmware/firmware.uf2"
        firmware.write_bytes(b"current UF2 artifact")
        self.digest = hashlib.sha256(firmware.read_bytes()).hexdigest()
        self.args = SimpleNamespace(output_dir=self.output, build_only=False,
            board="raspberry-pi-pico", node="node", max_instructions=1000, timeout=1,
            rp2350js_dir=self.output,
            target={"cpu_model": "rp2040", "scenarios": {"pico_spi": {"checks": 4}}})
        self.report = {"scenario": "pico_spi", "status": "running"}
        evidence = json.loads((ROOT / "tests/rp2350js/model-limitations.json").read_text())
        self.dependency = {"revision": evidence["engine_revision"], "dist_sha256": evidence["engine_dist_sha256"], "node_minimum": 18}
        self.manifest = {"sdk": {"revision": evidence["sdk_revision"]}}
        self.runtime = result("pico_spi")
        self.runtime["firmware_sha256"] = self.digest
        self.runner_status = 0
        self.write_report = True
        self.node_version = "v24.0.0\n"
        for attribute, replacement in (
                ("build_artifacts", self.build), ("run", self.run_command),
                ("engine_directory", lambda _: (self.output, self.dependency))):
            patcher = patch.object(adapter, attribute, replacement)
            patcher.start(); self.addCleanup(patcher.stop)
        patcher = patch.object(adapter.shutil, "which", return_value="/usr/bin/node")
        patcher.start(); self.addCleanup(patcher.stop)

    def build(self, args, report, scenario):
        report.update(build_status="passed", runtime_status="not_run")
        return self.output, self.manifest, {}

    def run_command(self, command, **kwargs):
        if "--version" in command:
            return 0, self.node_version
        if self.write_report:
            (self.output / "runtime.json").write_text(json.dumps(self.runtime))
        return self.runner_status, "runner output"

    def test_verified_complete_runtime_passes(self):
        adapter.test(self.args, self.report)
        self.assertEqual(self.report["runtime_status"], "passed")

    def test_build_only_erases_previous_runtime_and_does_not_find_engine(self):
        for filename in ("runtime.json", "rp2350js.log", "runner.json"):
            (self.output / filename).write_text("old successful evidence")
        self.args.build_only = True
        with patch.object(adapter, "engine_directory", side_effect=AssertionError("must not find simulator")):
            adapter.test(self.args, self.report)
        self.assertEqual(self.report["runtime_status"], "not_run")
        self.assertFalse((self.output / "runtime.json").exists())
        self.assertFalse((self.output / "rp2350js.log").exists())

    def test_missing_new_runtime_cannot_reuse_older_success(self):
        (self.output / "runtime.json").write_text(json.dumps(self.runtime))
        self.write_report = False
        with self.assertRaisesRegex(ValueError, "runtime report"):
            adapter.test(self.args, self.report)
        self.assertNotEqual(self.report.get("runtime_status"), "passed")

    def test_wrong_firmware_cpu_and_boot_metadata_are_rejected(self):
        for field, value in (("firmware_sha256", "old hash"), ("cpu_model", "rp2350-arm"),
                             ("boot", "forced-PC"), ("sets_pc_sp", True), ("sets_vtor", True)):
            original = self.runtime[field]; self.runtime[field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "does not match"):
                adapter.test(self.args, self.report)
            self.runtime[field] = original

    def test_process_failure_and_host_limits_are_not_model_limitations(self):
        self.runner_status = 1
        with self.assertRaisesRegex(ValueError, "did not complete"):
            adapter.test(self.args, self.report)
        self.runner_status = 0
        for reason in ("host_timeout", "instruction_limit", "engine_error"):
            self.runtime["reason"] = reason
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, "did not complete"):
                adapter.test(self.args, self.report)
            self.assertNotEqual(self.report.get("runtime_status"), "model_limited")

    def test_missing_engine_or_node_remains_an_environment_failure(self):
        with patch.object(adapter, "engine_directory", side_effect=ValueError("rp2350js not built")), \
             self.assertRaisesRegex(ValueError, "not built"):
            adapter.test(self.args, self.report)
        with patch.object(adapter.shutil, "which", return_value=None), \
             self.assertRaisesRegex(ValueError, "Node.js not found"):
            adapter.test(self.args, self.report)
        self.assertNotEqual(self.report.get("runtime_status"), "model_limited")

    def test_unsupported_node_version_is_rejected_before_the_runner(self):
        self.node_version = "v17.0.0\n"
        with self.assertRaisesRegex(ValueError, "Node.js"):
            adapter.test(self.args, self.report)
        self.assertFalse((self.output / "runtime.json").exists())

    def test_firmware_defects_never_pass_even_if_done_and_check_count_match(self):
        self.runtime["observations"]["emela_board_failures"] = 1
        with self.assertRaises(ValueError):
            adapter.test(self.args, self.report)

    def test_cs_failure_on_supported_cpu_is_a_real_failure(self):
        self.runtime["spi_transfers"][2]["cs_enabled"] = False
        with self.assertRaisesRegex(ValueError, "physical observations failed"):
            adapter.test(self.args, self.report)


if __name__ == "__main__":
    unittest.main()
