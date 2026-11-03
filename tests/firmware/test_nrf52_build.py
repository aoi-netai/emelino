"""Reject unsupported Nordic hardware descriptions before invoking the compiler."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from firmware.build import cli
from firmware.build.profiles import load_profiles
from firmware.ports.nrf52.build import build, configure, validate


class NordicBuildTests(unittest.TestCase):
    def setUp(self):
        self.board, self.chip, self.directory = load_profiles("arduino-nano-33-ble")
        self.temporary = tempfile.TemporaryDirectory(prefix="emelino-nrf52-build-")
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)

    def test_gpio_only_profile_does_not_require_serial_mappings(self):
        board = dict(self.board, peripherals={})
        configure(self.output, board, self.chip, 8192, 64)
        self.assertNotIn("EMELA_SPI1", (self.output / "board_config.h").read_text())
        self.assertIn("ORIGIN = 0x0", (self.output / "firmware.ld").read_text())
        self.assertIn("UARTE0_UART0_IRQHandler", (self.output / "board_vectors.inc").read_text())

    def test_pin_mappings_can_move_between_ports(self):
        board = copy.deepcopy(self.board)
        board["peripherals"]["i2c1"] = {"scl_pin": 43, "sda_pin": 44}
        configure(self.output, board, self.chip, 16384, 256)
        self.assertIn("#define EMELA_I2C1_SDA_PIN 44", (self.output / "board_config.h").read_text())

    def test_reserved_and_out_of_package_pins_are_rejected(self):
        for pin in (0, 9, 18, 48, -1, True):
            board = copy.deepcopy(self.board)
            board["peripherals"]["spi1"]["sck_pin"] = pin
            with self.subTest(pin=pin), self.assertRaisesRegex(ValueError, "not available"):
                validate(board, self.chip, 16384, 256)
        board = copy.deepcopy(self.board)
        board["peripherals"]["uart0"]["rx_pin"] = board["peripherals"]["uart0"]["tx_pin"]
        with self.assertRaisesRegex(ValueError, "distinct"):
            validate(board, self.chip, 16384, 256)

    def test_clock_memory_and_stack_cannot_exceed_port_capabilities(self):
        board = copy.deepcopy(self.board)
        board["clock"]["frequency_hz"] = 32000000
        with self.assertRaisesRegex(ValueError, "64 MHz"):
            validate(board, self.chip, 16384, 256)
        for field, value in (("flash_origin", 0x10000), ("ram_bytes", 0x80000)):
            chip = dict(self.chip, **{field: value})
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "raw Flash"):
                validate(self.board, chip, 16384, 256)
        with self.assertRaisesRegex(ValueError, "exceeds RAM"):
            validate(self.board, self.chip, 0x40000, 256)

    def test_missing_service_mapping_and_unknown_controller_fail_before_sdk(self):
        args = cli.parser().parse_args(["--board", self.board["board"], "--generated-dir", str(self.output / "generated"),
                                       "--output-dir", str(self.output)])
        board = dict(self.board, peripherals={})
        with self.assertRaisesRegex(ValueError, "no mapping"):
            build(args, board, self.chip, self.directory, {}, {"services": ["service.uart"]},
                  lambda *args: self.fail("compiler must not run"))
        board = copy.deepcopy(self.board)
        board["peripherals"]["uart1"] = board["peripherals"].pop("uart0")
        with self.assertRaisesRegex(ValueError, "only uart0"):
            validate(board, self.chip, 16384, 256)


if __name__ == "__main__":
    unittest.main()
