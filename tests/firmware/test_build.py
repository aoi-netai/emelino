"""Build-contract regressions independent of a compiler or vendor SDK."""

import copy
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from firmware.build import cli
from firmware.build.profiles import load_profiles
from firmware.build.support import record_artifacts, sha256
from firmware.ports.stm32f4.build import build as build_stm32f4
from firmware.ports.stm32f4.build import configure, validate


class FirmwareBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="emelino-build-contract-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.board, self.chip, self.board_dir = load_profiles("nucleo-f446re")

    def test_new_board_reuses_port_and_chip_without_peripheral_mappings(self):
        registry = self.root / "firmware"
        board_dir = registry / "boards/another-f446re"
        board_dir.mkdir(parents=True)
        (registry / "chips").mkdir()
        (registry / "ports/stm32f4").mkdir(parents=True)
        (registry / "ports/stm32f4/build.py").write_text("")
        board = dict(self.board, board="another-f446re")
        board.pop("peripherals")
        (board_dir / "board.json").write_text(json.dumps(board))
        (registry / "chips/stm32f446re.json").write_text(json.dumps(self.chip))
        profile, chip, _ = load_profiles("another-f446re", registry)
        configure(self.root, profile, chip, 8192, 64)
        self.assertIn("#define EMELA_GPIO_PORT_COUNT 8", (self.root / "board_config.h").read_text())
        self.assertIn("0x2000", (self.root / "firmware.ld").read_text())
        self.assertNotIn("EMELA_UART2", (self.root / "board_config.h").read_text())

    def test_board_cannot_select_a_path_or_incompatible_chip(self):
        for board in ("../nucleo-f446re", "/tmp/board", "nucleo-f446re/extra"):
            with self.subTest(board=board), self.assertRaisesRegex(ValueError, "invalid board"):
                load_profiles(board)
        registry = self.root / "firmware"
        (registry / "boards/nucleo-f446re").mkdir(parents=True)
        (registry / "chips").mkdir()
        (registry / "boards/nucleo-f446re/board.json").write_text(json.dumps(self.board))
        chip = dict(self.chip, port="another_sdk")
        (registry / "chips/stm32f446re.json").write_text(json.dumps(chip))
        with self.assertRaisesRegex(ValueError, "disagree"):
            load_profiles("nucleo-f446re", registry)

    def test_unsupported_clock_or_controller_is_not_silently_accepted(self):
        board = copy.deepcopy(self.board)
        for frequency in (180000000, 16000000.0):
            board["clock"]["frequency_hz"] = frequency
            with self.subTest(frequency=frequency), self.assertRaisesRegex(ValueError, "reset HSI"):
                validate(board, self.chip, 16384, 256)
        board = copy.deepcopy(self.board)
        board["peripherals"]["spi2"] = board["peripherals"].pop("spi1")
        with self.assertRaisesRegex(ValueError, "only uart2, spi1 and i2c1"):
            validate(board, self.chip, 16384, 256)

    def test_required_service_rejects_null_or_missing_mapping_without_invoking_tools(self):
        args = cli.parser().parse_args(["--generated-dir", str(self.root / "generated"),
                                       "--output-dir", str(self.root)])
        for mapping, message in ((None, "only uart2, spi1 and i2c1"), ({}, "no mapping for required service")):
            board = dict(self.board, peripherals=mapping)
            with self.subTest(mapping=mapping), self.assertRaisesRegex(ValueError, message):
                build_stm32f4(args, board, self.chip, self.board_dir, {},
                              {"services": ["service.spi"]},
                              lambda *args: self.fail("must reject configuration before invoking tools"))

    def test_invalid_package_pins_and_stack_reservation_fail_before_compile(self):
        board = copy.deepcopy(self.board)
        board["peripherals"]["spi1"]["sck_pin"] = 160
        with self.assertRaisesRegex(ValueError, "not available"):
            validate(board, self.chip, 16384, 256)
        with self.assertRaisesRegex(ValueError, "exceeds RAM"):
            validate(self.board, self.chip, self.chip["ram_bytes"], 256)

    def artifact(self):
        generated = self.root / "generated"
        generated.mkdir()
        files = {"app.c": "source", "emela_embedded.h": "public_header",
                 "emela_board.h": "platform_header", "emela_embedded_exports.h": "aliases_header"}
        for name in files:
            (generated / name).write_text("")
        (generated / "emela_artifact.json").write_text(json.dumps({
            "schema_version": 2, "abi_version": 2,
            "files": [{"path": name, "role": role} for name, role in files.items()],
            "metadata": {"entry": "main", "exports": [], "services": [],
                         "external_symbols": [], "isr_entries": [], "isr_safe_symbols": [],
                         "task_capacity": 0}}))
        return generated

    def test_failed_rebuild_removes_previous_success_marker(self):
        generated = self.artifact()
        output = self.root / "firmware"
        output.mkdir()
        (output / "firmware.json").write_text('{"status":"previous success"}')
        args = cli.parser().parse_args(["--generated-dir", str(generated), "--output-dir", str(output)])
        def fail_build(*args):
            raise ValueError("SDK failed")

        backend = SimpleNamespace(build=fail_build)
        with patch("firmware.build.cli.importlib.import_module", return_value=backend):
            with self.assertRaisesRegex(ValueError, "SDK failed"):
                cli.build(args)
        self.assertFalse((output / "firmware.json").exists())
        # Profile errors also invalidate success from an earlier build.
        (output / "firmware.json").write_text("{}")
        args.board = "../bad"
        with self.assertRaisesRegex(ValueError, "invalid board"):
            cli.build(args)
        self.assertFalse((output / "firmware.json").exists())

    def test_result_hashes_all_outputs_and_refuses_missing_or_escaping_artifacts(self):
        output = self.root / "firmware"
        output.mkdir()
        (output / "app.bin").write_bytes(b"image")
        with self.assertRaises(OSError):
            record_artifacts(output, {"artifacts": ["app.bin", "missing.map"]})
        self.assertFalse((output / "firmware.json").exists())
        for name in ("../outside.bin", str(self.root / "outside.bin")):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "escapes"):
                record_artifacts(output, {"artifacts": [name]})
        manifest = record_artifacts(output, {"artifacts": ["app.bin"], "board": "test"})
        self.assertEqual(manifest["sha256"]["app.bin"], sha256(output / "app.bin"))
        self.assertEqual(json.loads((output / "firmware.json").read_text()), manifest)


if __name__ == "__main__":
    unittest.main()
