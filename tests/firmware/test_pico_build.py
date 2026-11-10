"""Pico board and SDK build boundary regressions; no vendor tools required."""
import copy
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from firmware.build import cli
from firmware.build.profiles import load_profiles
from firmware.build.support import record_artifacts
from firmware.ports.pico_sdk import build


class PicoBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)

    def test_both_boards_have_independent_platform_and_memory(self):
        for name, platform, ram in (("raspberry-pi-pico", "rp2040", 264 * 1024),
                                     ("raspberry-pi-pico-2", "rp2350-arm-s", 520 * 1024)):
            with self.subTest(board=name):
                board, chip, _ = load_profiles(name)
                build.configure(self.output, board, chip, 16384, 256, True)
                self.assertEqual(board["pico_platform"], platform)
                script = (self.output / "linker/set_memory_locations.incl").read_text()
                self.assertIn(f"RAM_LENGTH = {ram - 16384 - 256:#x};", script)
                self.assertIn(f"SCRATCH_Y_ORIGIN = {0x20000000 + ram - 16384:#x};", script)
                header = (self.output / "board_config.h").read_text()
                self.assertIn("EMELA_SPI1_SCK_PIN 18", header)
                self.assertIn("EMELA_PERIPHERAL_TIME 1", header)

    def test_mismatched_cpu_board_and_flash_are_rejected(self):
        board, chip, _ = load_profiles("raspberry-pi-pico-2")
        for key, wrong in (("pico_board", "pico"), ("pico_platform", "rp2350-riscv"),
                           ("flash_bytes", 2 * 1024 * 1024), ("gpio_pin_mask", 0xffffffff),
                           ("clock", {"source": "sdk", "frequency_hz": 125000000})):
            invalid = dict(board, **{key: wrong})
            with self.subTest(key=key), self.assertRaises(ValueError):
                build.validate(invalid, chip, 16384, 256)
        with self.assertRaisesRegex(ValueError, "CPU mismatch"):
            build.validate(board, dict(chip, cpu="cortex-m0plus"), 16384, 256)

    def test_pin_mapping_and_stack_fail_before_tools(self):
        board, chip, board_dir = load_profiles("raspberry-pi-pico")
        for stack, guard in ((0, 256), (16385, 256), (16384, 0), (264 * 1024, 256)):
            with self.subTest(stack=stack, guard=guard), self.assertRaises(ValueError):
                build.validate(board, chip, stack, guard)
        invalid = copy.deepcopy(board)
        invalid["peripherals"]["uart0"]["tx_pin"] = 23
        with self.assertRaisesRegex(ValueError, "pin mapping"):
            build.validate(invalid, chip, 16384, 256)
        args = cli.parser().parse_args(["--generated-dir", str(self.output / "generated"),
                                       "--output-dir", str(self.output)])
        for mapping in ({}, None):
            with self.subTest(mapping=mapping), self.assertRaises(ValueError):
                build.build(args, dict(board, peripherals=mapping), chip, board_dir, {},
                            {"services": ["service.spi"]}, lambda *_: self.fail("invoked tool"))

    def test_profile_numbers_reject_equal_valued_floats_and_booleans(self):
        board, chip, _ = load_profiles("raspberry-pi-pico")
        for key in ("flash_bytes", "gpio_pin_mask"):
            with self.subTest(profile=key), self.assertRaises(ValueError):
                build.validate(dict(board, **{key: float(board[key])}), chip, 16384, 256)
        for key in ("flash_origin", "ram_origin", "ram_bytes", "gpio_pin_mask", "analog_pin_mask"):
            with self.subTest(chip=key), self.assertRaises(ValueError):
                build.validate(board, dict(chip, **{key: float(chip[key])}), 16384, 256)
        with self.assertRaises(ValueError):
            build.validate(dict(board, clock={"source": "sdk", "frequency_hz": 125000000.0}), chip, 16384, 256)
        wrong = copy.deepcopy(board)
        wrong["peripherals"]["uart0"]["rx_pin"] = True
        with self.assertRaises(ValueError):
            build.validate(wrong, chip, 16384, 256)

    def test_cmake_values_cannot_expand_code_or_split_lists(self):
        self.assertEqual(build.cmake_value('a ${x} " path'), '[=[a ${x} " path]=]')
        self.assertEqual(build.cmake_value('a]=]b'), '[==[a]=]b]==]')
        for value in ("a;b", "a\nb", "a\0b"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                build.cmake_value(value)

    def test_missing_sdk_fails_with_actionable_diagnostic(self):
        args = cli.parser().parse_args(["--generated-dir", str(self.output),
                                       "--output-dir", str(self.output / "out"),
                                       "--sdk-dir", str(self.output / "missing")])
        with patch.dict("os.environ", {}, clear=True), self.assertRaisesRegex(ValueError, "setup/pico-sdk.sh"):
            build.sdk_paths(args)

    def test_malformed_sdk_version_is_an_actionable_error(self):
        (self.output / "pico_sdk_init.cmake").write_text("")
        (self.output / "pico_sdk_version.cmake").write_text("set(PICO_SDK_VERSION_MAJOR 2)\n")
        args = cli.parser().parse_args(["--generated-dir", str(self.output),
            "--output-dir", str(self.output / "out"), "--sdk-dir", str(self.output)])
        with self.assertRaisesRegex(ValueError, "malformed Pico SDK version"):
            build.sdk_paths(args)

    def test_sdk_identity_rejects_wrong_revision_and_tracked_changes(self):
        def git(*arguments):
            return subprocess.check_output(["git", "-C", str(self.output), *arguments],
                                           stderr=subprocess.STDOUT, text=True).strip()
        git("init", "--quiet")
        source = self.output / "pico_sdk_version.cmake"
        source.write_text("original")
        git("add", source.name)
        git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")
        revision = git("rev-parse", "HEAD")
        with self.assertRaisesRegex(ValueError, "checkout must be at"):
            build.sdk_identity(self.output)
        with patch.object(build, "SDK_REVISION", revision):
            self.assertFalse(build.sdk_identity(self.output)["dirty"])
            source.write_text("changed")
            with self.assertRaisesRegex(ValueError, "tracked modifications"):
                build.sdk_identity(self.output)

    def test_archive_provenance_hashes_source_changes(self):
        self.assertEqual(build.sdk_identity(self.output)["source_kind"], "archive")
        source = self.output / "input.h"
        source.write_text("before")
        first = build.sdk_source_hashes(self.output)
        source.write_text("after")
        self.assertNotEqual(first["input.h"], build.sdk_source_hashes(self.output)["input.h"])
        (self.output / ".git").mkdir()
        (self.output / ".git/config").write_text("metadata")
        self.assertEqual(set(build.sdk_source_hashes(self.output)), {"input.h"})

    def test_missing_new_uf2_cannot_reuse_an_older_successful_output(self):
        board, chip, board_dir = load_profiles("raspberry-pi-pico")
        args = cli.parser().parse_args(["--generated-dir", str(self.output / "generated"),
                                       "--output-dir", str(self.output)])
        sdk = self.output / "sdk"
        sdk.mkdir()
        (sdk / "pico_sdk_init.cmake").write_text("fixture")
        picotool = self.output / "picotool"
        picotool.write_text("fixture")
        compiler = Path("/usr/bin/cc").resolve()
        (self.output / "firmware.uf2").write_bytes(b"previous successful UF2")

        def run(command, **_):
            if str(command[0]) == str(picotool):
                return "picotool v2.3.1 (fixture)\n"
            if "--version" in command:
                return "fixture version\n"
            if "-S" in command:
                self.assertFalse((self.output / "firmware.uf2").exists())
                (self.output / "compiler-path.txt").write_text(str(compiler))
                (self.output / "picotool-path.txt").write_text(str(picotool))
            if "--build" in command:
                for name in ("firmware.elf", "firmware.bin", "firmware.map"):
                    (self.output / name).write_bytes(b"new build, UF2 unexpectedly omitted")
            return ""

        with patch.object(build, "sdk_paths", return_value=(sdk, self.output)), \
             patch.object(build, "compiler_path", return_value=compiler), \
             patch.object(build, "inspect_elf", return_value={}):
            result = build.build(args, board, chip, board_dir, {"source": []}, {"services": []}, run)
        with self.assertRaises(FileNotFoundError):
            record_artifacts(self.output, result)
        self.assertFalse((self.output / "firmware.json").exists())

    def test_wrong_or_truncated_elf_is_rejected(self):
        path = self.output / "firmware.elf"
        for content in (b"", b"not ELF", b"\x7fELF\x02\x01\x01" + bytes(45),
                        b"\x7fELF\x01\x01\x01" + bytes(45)):
            path.write_bytes(content)
            with self.subTest(content=content[:7]), self.assertRaises(ValueError):
                build.elf_layout(path)


if __name__ == "__main__":
    unittest.main()
