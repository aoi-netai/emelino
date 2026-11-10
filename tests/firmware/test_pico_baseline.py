"""Plain SDK reproduction must use the same setup environment as firmware builds."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("pico_baseline", ROOT / "tests/rp2350js/baseline.py")
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class BaselineEnvironmentTests(unittest.TestCase):
    def test_setup_custom_prefix_and_explicit_override_reach_cmake(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sdk = root / "sdk"
            (sdk / "external").mkdir(parents=True)
            (sdk / "external/pico_sdk_import.cmake").touch()
            installed = root / "custom prefix/picotool-2.3.1/picotool"
            explicit = root / "explicit picotool"
            for directory in (installed, explicit):
                directory.mkdir(parents=True)
                (directory / "picotoolConfig.cmake").touch()
            for variable, override in (("picotool_DIR", False), ("PICOTOOL_DIR", False),
                                       ("picotool_DIR", True)):
                expected = explicit if override else installed
                argv = ["baseline.py", "--board", "raspberry-pi-pico", "--output-dir", str(root / "out")]
                if override:
                    argv += ["--picotool-dir", str(explicit)]
                environment = {"PICO_SDK_PATH": str(sdk), variable: str(installed)}
                # Stop at configure: this test checks environment/CLI plumbing,
                # without compiling or downloading any dependency.
                with self.subTest(variable=variable, override=override), \
                     patch.dict(os.environ, environment, clear=True), patch.object(sys, "argv", argv), \
                     patch.object(baseline, "compiler_path", return_value=Path("/opt/toolchain/bin/arm-none-eabi-gcc")), \
                     patch.object(baseline, "run", return_value=(1, "stop at configure")) as run, \
                     redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                    self.assertEqual(baseline.main(), 1)
                    run.assert_called_once()
                    self.assertIn("-Dpicotool_DIR=" + str(expected), run.call_args.args[0])


class BaselineBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sdk = self.root / "sdk"
        (self.sdk / "external").mkdir(parents=True)
        (self.sdk / "external/pico_sdk_import.cmake").touch()
        self.picotool = self.root / "picotool"
        self.picotool.mkdir()
        (self.picotool / "picotoolConfig.cmake").touch()
        self.output = self.root / "output"
        self.output.mkdir()
        self.compiler = Path("/opt/first/bin/arm-none-eabi-gcc")
        self.actual_compiler = None
        self.build_calls = 0
        self.runner_calls = 0
        self.omit_uf2 = False
        self.evidence = json.loads((ROOT / "tests/rp2350js/model-limitations.json").read_text())
        self.control = self.evidence["baselines"]["raspberry-pi-pico"]
        self.observations = dict(self.control["observations"])

    def fake_run(self, command, **kwargs):
        command = list(map(str, command))
        if "-S" in command:
            tree = Path(command[command.index("-B") + 1])
            # A second compiler must get neither the first cache nor its objects.
            self.assertEqual(tree, self.output / "sdk-build")
            self.assertFalse(tree.exists())
            tree.mkdir()
            selected = self.actual_compiler or self.compiler
            (tree / "CMakeCache.txt").write_text(f"CMAKE_C_COMPILER:FILEPATH={selected}\n")
            self.assertIn("-DPICO_TOOLCHAIN_PATH=" + str(self.compiler.parent.parent), command)
            return 0, "configured"
        if "--build" in command:
            self.build_calls += 1
            tree = Path(command[command.index("--build") + 1])
            (tree / "previous-object.o").write_bytes(b"object")
            (tree / "pico_sdk_baseline.elf").write_bytes(str(self.compiler).encode())
            if not self.omit_uf2:
                (tree / "pico_sdk_baseline.uf2").write_bytes(str(self.compiler).encode())
            return 0, "built"
        if "--defined-only" in command:
            return 0, "20000000 B pico_baseline_done\n"
        if "--version" in command:
            return 0, "test compiler\n"
        if "rev-parse" in command:
            return 0, self.evidence["sdk_revision"] + "\n"
        self.assertEqual(Path(command[1]), ROOT / "tests/rp2350js/run.cjs")
        self.runner_calls += 1
        config = json.loads(Path(command[2]).read_text())
        result = {"reason": "completed", "uart": "SDK\n", "observations": self.observations,
                  "gpio25": self.control["gpio25"]}
        Path(config["report"]).write_text(json.dumps(result))
        return 0, "executed"

    def execute(self, *extra):
        dependency = {"revision": self.evidence["engine_revision"],
                      "dist_sha256": self.evidence["engine_dist_sha256"]}
        adapter = SimpleNamespace(engine_directory=lambda _: (self.root, dependency))
        limits = SimpleNamespace(evidence_for=lambda *_: self.evidence)
        argv = ["baseline.py", "--board", "raspberry-pi-pico", "--sdk-dir", str(self.sdk),
                "--picotool-dir", str(self.picotool), "--output-dir", str(self.output), *extra]
        with patch.object(sys, "argv", argv), \
             patch.object(baseline, "compiler_path", return_value=self.compiler), \
             patch.object(baseline, "run", side_effect=self.fake_run), \
             patch.object(baseline, "load_module", side_effect=[adapter, limits]), \
             redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            status = baseline.main()
        return status, json.loads((self.output / "report.json").read_text())

    def test_changing_toolchain_uses_a_fresh_tree_and_preserves_other_files(self):
        keep = self.output / "notes.txt"
        keep.write_text("caller file")
        old_map = self.output / "pico_sdk_baseline.elf.map"
        old_map.write_text("previous map")
        self.assertEqual(self.execute()[0], 0)
        self.compiler = Path("/opt/second/bin/arm-none-eabi-gcc")
        self.assertEqual(self.execute()[0], 0)
        self.assertEqual(self.build_calls, 2)
        self.assertEqual((self.output / "pico_sdk_baseline.elf").read_bytes(), str(self.compiler).encode())
        self.assertEqual(keep.read_text(), "caller file")
        self.assertFalse(old_map.exists())

    def test_unexpected_sdk_compiler_fails_before_build(self):
        self.actual_compiler = Path("/opt/old/bin/arm-none-eabi-gcc")
        status, report = self.execute()
        self.assertEqual(status, 1)
        self.assertIn("SDK selected", report["error"])
        self.assertEqual(self.build_calls, 0)

    def test_omitted_new_uf2_cannot_reuse_old_image(self):
        self.assertEqual(self.execute()[0], 0)
        self.omit_uf2 = True
        status, report = self.execute()
        self.assertEqual(status, 1)
        self.assertEqual(report["build_status"], "failed")
        self.assertEqual(report["runtime_status"], "not_run")
        self.assertEqual(self.runner_calls, 1)
        self.assertFalse((self.output / "pico_sdk_baseline.uf2").exists())
        self.assertFalse((self.output / "runtime.json").exists())

    def test_optimization_timing_variation_preserves_only_known_limitations(self):
        self.observations["pico_baseline_elapsed"] = 101
        status, report = self.execute("--optimization", "O0")
        self.assertEqual(status, 0)
        self.assertEqual(report["runtime_status"], "model_limited")
        self.assertEqual(report["runtime"]["observations"]["pico_baseline_elapsed"], 101)
        self.observations["pico_baseline_elapsed"] = 99
        self.assertEqual(self.execute("--optimization", "O0")[0], 1)

    def test_timing_tolerance_does_not_hide_other_observation_changes(self):
        self.observations["pico_baseline_elapsed"] = 101
        for name in ("pico_baseline_done", "pico_baseline_stage", "pico_baseline_callbacks",
                     "pico_baseline_uart_rsr", "pico_baseline_i2c_enable_status"):
            original = self.observations[name]
            self.observations[name] ^= 1
            with self.subTest(name=name):
                status, report = self.execute("--optimization", "O0")
                self.assertEqual(status, 1)
                self.assertEqual(report["runtime_status"], "failed")
            self.observations[name] = original


if __name__ == "__main__":
    unittest.main()
