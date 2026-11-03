"""Failures must not leave a passing report or silently select another target."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests/embedded"))
from support.common import target_source


class RunnerFailureTest(unittest.TestCase):
    def invoke(self, *args):
        return subprocess.run([sys.executable, str(ROOT / "tools/test_embedded.py"), *map(str, args)],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def test_unknown_scenario_is_rejected_before_build(self):
        result = self.invoke("--scenario", "missing")
        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown or duplicate scenario", result.stderr)

    def test_target_cannot_silently_select_a_different_board(self):
        target = json.loads((ROOT / "tests/embedded/targets/nucleo-f446re.json").read_text())
        target["board"] = "another-board"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "target.json"
            path.write_text(json.dumps(target))
            result = self.invoke("--target", path, "--scenario", "bmp180")
        self.assertEqual(result.returncode, 2)
        self.assertIn("does not match", result.stderr)

    def test_failed_build_replaces_old_success_and_matrix_continues(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            for name in ("bmp180", "mt25q"):
                (output / name).mkdir()
                (output / name / "report.json").write_text('{"status":"passed"}')
            # A real existing executable satisfies discovery; failure occurs at
            # the missing compiler before any Renode or SDK invocation.
            result = self.invoke("--scenario", "bmp180", "--scenario", "mt25q",
                                 "--output-dir", output, "--emelino", output / "missing-emelino",
                                 "--renode", sys.executable)
            self.assertEqual(result.returncode, 1)
            summary = json.loads((output / "report.json").read_text())
            self.assertEqual(summary["status"], "failed")
            self.assertEqual([case["scenario"] for case in summary["scenarios"]], ["bmp180", "mt25q"])
            for name in ("bmp180", "mt25q"):
                report = json.loads((output / name / "report.json").read_text())
                self.assertEqual(report["status"], "failed")
                self.assertIn("missing-emelino", report["error"])

    def test_combined_target_module_compiles_as_emela(self):
        compiler = ROOT / "target/debug/emelino"
        if not compiler.is_file():
            self.skipTest("build emelino-cli to check generated module syntax and types")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / "test").mkdir()
            (output / "test/target.emel").write_text(target_source({
                "i2c": 1, "spi": 1, "cs_pin": 16, "address": 118, "missing_address": 117}))
            (output / "main.emel").write_text('''import Test.Target
pub fn main() -> Int {
  bus = Target.i2c()
  serial = Target.spi()
  pin = Target.chip_select()
  Target.address() + Target.missing_address()
}
''')
            config = {"entry": "main", "exports": [{"function": "main", "symbol": "target_module_check"}],
                      "sources": ["test/target.emel"]}
            (output / "config.json").write_text(json.dumps(config))
            result = subprocess.run([str(compiler), "build", str(output / "main.emel"),
                "--config", str(output / "config.json"), "--out-dir", str(output / "generated")],
                capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
