"""Board contracts and failure boundaries, without invoking vendor tools."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from firmware.build import cli
from firmware.build.profiles import load_profiles
from firmware.ports.esp_idf import build

BOARD = "esp32-c3-devkitm-1-n4x"


class EspIdfBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)
        self.profile, self.chip, self.board_dir = load_profiles(BOARD)

    def test_flash_pins_are_excluded_and_sdk_owns_the_cpu(self):
        build.configure(self.output, self.profile, self.chip, 16384, 256)
        for pin in range(22):
            self.assertEqual(bool(self.profile["gpio_pin_mask"] & (1 << pin)), pin <= 10 or pin >= 18)
        self.assertEqual(self.chip["cpu"], "rv32imc")
        header = (self.output / "board_config.h").read_text()
        self.assertIn("EMELA_UART0_TX_PIN 21", header)
        self.assertIn("EMELA_SPI1_SCK_PIN 4", header)
        self.assertIn("EMELA_STACK_GUARD_BYTES 256", header)

    def test_memory_clock_and_cpu_mismatches_are_rejected(self):
        for key, wrong in (("flash_bytes", 2097152), ("gpio_pin_mask", 0x3fffff),
                           ("clock", {"source": "sdk", "frequency_hz": 80000000})):
            with self.subTest(key=key), self.assertRaises(ValueError):
                build.validate(dict(self.profile, **{key: wrong}), self.chip, 16384, 256)
        for key, wrong in (("cpu", "cortex-m4"), ("idf_target", "esp32"), ("ram_bytes", 520192)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                build.validate(self.profile, dict(self.chip, **{key: wrong}), 16384, 256)

    def test_stack_and_guard_reservations_are_not_silently_ignored(self):
        for stack, guard in ((0, 256), (4095, 256), (4097, 256), (16384, 0), (16384, 33), (409600, 256)):
            with self.subTest(stack=stack, guard=guard), self.assertRaises(ValueError):
                build.validate(self.profile, self.chip, stack, guard)

    def test_unexposed_duplicate_and_nonintegral_pin_maps_fail_before_tools(self):
        for wrong in (11, 12, 17, 22, True, 21.0, 20):
            profile = copy.deepcopy(self.profile)
            profile["peripherals"]["uart0"]["tx_pin"] = wrong
            with self.subTest(pin=wrong), self.assertRaises(ValueError):
                build.validate(profile, self.chip, 16384, 256)

    def test_cmake_quoting_preserves_literal_paths_and_rejects_lists(self):
        self.assertEqual(build.cmake_value('${value} " $ path'), '[=[${value} " $ path]=]')
        self.assertEqual(build.cmake_value('x]=]y'), '[==[x]=]y]==]')
        for value in ("a;b", "a\nb", "a\rb", "a\0b"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                build.cmake_value(value)

    def test_missing_sdk_reports_the_setup_command(self):
        args = cli.parser().parse_args(["--generated-dir", str(self.output), "--output-dir", str(self.output / "out"),
                                       "--sdk-dir", str(self.output / "missing")])
        with self.assertRaisesRegex(ValueError, "setup/esp32-c3-devkitm-1-n4x.sh"):
            build.sdk_path(args)

    def test_cleanup_cannot_remove_input_tree_or_symlink_target(self):
        tree = self.output / "sdk-build"
        tree.mkdir()
        file = tree / "source.c"
        file.write_text("caller-owned source")
        alias = self.output / "input.c"
        alias.symlink_to(file)
        for path in (tree, file, alias):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "overlaps"):
                build.reject_tree_input(tree, path)
        self.assertEqual(file.read_text(), "caller-owned source")
        self.assertTrue(alias.is_symlink())

    def test_missing_service_wiring_is_rejected_without_invoking_tools(self):
        args = cli.parser().parse_args(["--generated-dir", str(self.output / "generated"), "--output-dir", str(self.output)])
        profile = dict(self.profile, peripherals={})
        for driver in ("uart", "spi", "i2c"):
            run = Mock(side_effect=AssertionError("invoked external tool"))
            with self.subTest(driver=driver), self.assertRaisesRegex(ValueError, "no mapping"):
                build.build(args, profile, self.chip, self.board_dir, {"source": []}, {"services": ["service." + driver]}, run)
            run.assert_not_called()

    def test_wrong_sdk_revision_and_local_changes_fail(self):
        (self.output / ".git").mkdir()
        def git(command, **kwargs):
            if command[-2:] == ["rev-parse", "HEAD"]:
                return "different revision\n"
            return ""
        with patch.object(build.subprocess, "check_output", side_effect=git), self.assertRaisesRegex(ValueError, "checkout must be"):
            build.sdk_identity(self.output)
        with patch.object(build.subprocess, "check_output", side_effect=[build.SDK_REVISION, " M tools/cmake/project.cmake"]), \
             self.assertRaisesRegex(ValueError, "tracked modifications"):
            build.sdk_identity(self.output)

    def test_idf_entry_name_cannot_collide_with_generated_exports(self):
        args = cli.parser().parse_args(["--generated-dir", str(self.output / "generated"), "--output-dir", str(self.output)])
        run = Mock(side_effect=AssertionError("invoked external tool"))
        with self.assertRaisesRegex(ValueError, "ESP-IDF owns app_main"):
            build.build(args, self.profile, self.chip, self.board_dir, {},
                        {"exports": [{"c_symbol": "app_main"}], "services": []}, run)
        run.assert_not_called()

    def test_wrong_architecture_or_truncated_elf_is_rejected(self):
        elf = self.output / "firmware.elf"
        for data in (b"", b"not ELF", b"\x7fELF\x02\x01\x01" + bytes(45), b"\x7fELF\x01\x01\x01" + bytes(45)):
            elf.write_bytes(data)
            with self.subTest(data=data[:7]), self.assertRaises(ValueError):
                build.elf_layout(elf)


if __name__ == "__main__":
    unittest.main()
