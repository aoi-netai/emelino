"""Exercise setup without apt, Internet access, or changes to the user's tools."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "tools/setup"

STUB = '''#!/usr/bin/python3
import json
import os
from pathlib import Path
import subprocess
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["SETUP_TEST_LOG"], "a") as log:
    log.write(json.dumps([name, *args]) + "\\n")
if name == "sudo":
    sys.exit(subprocess.run(args).returncode)
elif name == "curl":
    Path(args[args.index("-o") + 1]).write_text(
        '#!/bin/sh\\nmkdir -p "$CARGO_HOME/bin"\\n'
        'cp "$SETUP_TEST_RUSTUP" "$CARGO_HOME/bin/rustup"\\n'
        'chmod +x "$CARGO_HOME/bin/rustup"\\n')
elif name == "rustup":
    state = Path(os.environ["CARGO_HOME"]) / "stable-installed"
    if args[:2] == ["toolchain", "install"]:
        state.touch()
    elif not state.exists():
        sys.exit(1)
elif name == "arm-none-eabi-gcc":
    if os.environ.get("SETUP_TEST_FAIL_LINK"):
        sys.exit(1)
    Path(args[args.index("-o") + 1]).write_bytes(b"fake ELF")
'''


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="emelino-setup-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.prefix = self.root / "tools with 'quotes' $ and spaces"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls.jsonl"
        self.log.touch()
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin",
                        CARGO_HOME=str(self.root / "cargo home"),
                        RUSTUP_HOME=str(self.root / "rustup home"),
                        SETUP_TEST_LOG=str(self.log),
                        GIT_CONFIG_GLOBAL=str(self.root / "gitconfig"),
                        GIT_CONFIG_NOSYSTEM="1", GIT_ALLOW_PROTOCOL="file",
                        GIT_AUTHOR_NAME="Setup test", GIT_AUTHOR_EMAIL="setup@example.invalid",
                        GIT_COMMITTER_NAME="Setup test", GIT_COMMITTER_EMAIL="setup@example.invalid")
        for key in ("ARM_GCC", "RUSTUP_TOOLCHAIN", "GIT_CONFIG_COUNT", "GIT_DIR", "GIT_WORK_TREE"):
            self.env.pop(key, None)
        for name in ("sudo", "apt-get", "curl", "rustup", "arm-none-eabi-gcc",
                     "arm-none-eabi-nm", "arm-none-eabi-objcopy", "arm-none-eabi-size"):
            path = self.bin / name
            path.write_text(STUB)
            path.chmod(0o755)
        # rustup should be absent until the fake official installer runs.
        self.env["SETUP_TEST_RUSTUP"] = str(self.bin / "rustup-template")
        (self.bin / "rustup").rename(self.bin / "rustup-template")

    def command(self, *args, check=True, env=None):
        result = subprocess.run(args, cwd=self.root, env=env or self.env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        if check:
            self.assertEqual(result.returncode, 0, result.stdout)
        return result

    def setup(self, name, *args, check=True, env=None):
        return self.command("bash", str(SCRIPTS / f"{name}.sh"), "--prefix", str(self.prefix),
                            *args, check=check, env=env)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def repository(self, name, tag, files):
        path = self.root / "repositories" / name
        path.mkdir(parents=True)
        self.command("git", "init", "-q", str(path))
        for name, content in files.items():
            target = path / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
        self.command("git", "-C", str(path), "add", ".")
        self.command("git", "-C", str(path), "commit", "-qm", "fixture")
        if tag:
            self.command("git", "-C", str(path), "tag", tag)
        return path

    def board_repositories(self):
        hal = self.repository("hal", None, {"Src/stm32f4xx_hal_gpio.c": "HAL fixture"})
        device = self.repository("device", None, {"Include/stm32f446xx.h": "device fixture"})
        stm32 = self.repository("stm32", None, {"Drivers/CMSIS/Include/core_cm4.h": "core fixture"})
        for repo, destination in ((hal, "Drivers/STM32F4xx_HAL_Driver"),
                                  (device, "Drivers/CMSIS/Device/ST/STM32F4xx")):
            self.command("git", "-C", str(stm32), "submodule", "add", "-q", repo.as_uri(), destination)
        self.command("git", "-C", str(stm32), "commit", "-qam", "SDK submodules")
        self.command("git", "-C", str(stm32), "tag", "v1.28.3")
        nrfx = self.repository("nrfx", "v3.2.0", {"mdk/nrf52840.h": "Nordic fixture",
                                                   "mdk/system_nrf52840.c": "SystemInit fixture"})
        cmsis = self.repository("cmsis", "5.9.0", {"CMSIS/Core/Include/core_cm4.h": "Arm fixture"})
        for repo, url in ((stm32, "https://github.com/STMicroelectronics/STM32CubeF4.git"),
                          (nrfx, "https://github.com/NordicSemiconductor/nrfx.git"),
                          (cmsis, "https://github.com/ARM-software/CMSIS_5.git")):
            self.command("git", "config", "--global", f"url.{repo.as_uri()}.insteadOf", url)

    def source_environment(self):
        result = self.command("bash", "-c", 'source "$1"; printf "%s\\n" '
                              '"${ARM_GCC-}" "${STM32CUBE_F4-}" "${NRFX_ROOT-}" "${CMSIS_ROOT-}"',
                              "test", str(self.prefix / "env.sh"))
        return result.stdout.splitlines()

    def test_dry_runs_do_not_install_or_write_files(self):
        for script in ("common", "nucleo-f446re", "arduino-nano-33-ble", "pico-sdk", "rp2350js"):
            with self.subTest(script=script):
                result = self.setup(script, "--dry-run")
                self.assertIn("Dry run complete", result.stdout)
                self.assertFalse(self.prefix.exists())
                self.assertEqual(self.calls(), [])

    def test_pico_dry_runs_do_not_execute_optional_tools(self):
        # Installed, broken tools also must not be probed or run during --dry-run.
        for name in ("node", "npm", "cmake", "arm-none-eabi-gcc"):
            path = self.bin / name
            path.write_text(STUB + "\nraise SystemExit(91)\n")
            path.chmod(0o755)
        for name in ("pico-sdk", "rp2350js"):
            self.setup(name, "--dry-run")
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.prefix.exists())

    def test_pico_and_engine_environment_survive_other_board_setup_and_dry_run(self):
        self.command("bash", "-c", '''source "$1"
SETUP_PREFIX=$2
SETUP_DRY_RUN=0
write_environment pico-sdk PICO_SDK_PATH "$2/SDK with spaces" picotool_DIR "$2/picotool"
write_environment rp2350js RP2350JS_DIR "$2/engine with spaces"
''', "test", str(SCRIPTS / "lib.sh"), str(self.prefix))
        self.board_repositories()
        self.setup("arduino-nano-33-ble")
        snapshot = {path: path.read_bytes() for path in (self.prefix / "env.d").glob("*.sh")}
        snapshot[self.prefix / "env.sh"] = (self.prefix / "env.sh").read_bytes()
        for name in ("pico-sdk", "rp2350js"):
            self.setup(name, "--dry-run")
        self.assertEqual(snapshot, {path: path.read_bytes() for path in snapshot})
        result = self.command("bash", "-c", 'source "$1"; printf "%s\\n" '
            '"$PICO_SDK_PATH" "$picotool_DIR" "$RP2350JS_DIR" "$NRFX_ROOT"',
            "test", str(self.prefix / "env.sh"))
        self.assertEqual(result.stdout.splitlines(), [str(self.prefix / "SDK with spaces"),
            str(self.prefix / "picotool"), str(self.prefix / "engine with spaces"),
            str(self.prefix / "embedded-sdk/nrfx-3.2.0")])

    def test_common_installs_rustup_once_and_checks_both_c_targets(self):
        self.setup("common")
        self.setup("common")
        calls = self.calls()
        self.assertEqual(sum(call[0] == "curl" for call in calls), 1)
        self.assertEqual(sum(call[:3] == ["rustup", "toolchain", "install"] for call in calls), 2)
        arm_calls = [call for call in calls if call[0] == "arm-none-eabi-gcc"]
        self.assertEqual(len(arm_calls), 2)
        self.assertIn("--specs=nano.specs", arm_calls[0])
        self.assertIn("--specs=nosys.specs", arm_calls[0])
        self.assertEqual(self.source_environment()[0], str(self.bin / "arm-none-eabi-gcc"))
        restored = self.command("bash", "-c", 'unset CARGO_HOME RUSTUP_HOME; source "$1"; '
                                'printf "%s\\n" "$CARGO_HOME" "$RUSTUP_HOME"',
                                "test", str(self.prefix / "env.sh"))
        self.assertEqual(restored.stdout.splitlines(), [self.env["CARGO_HOME"], self.env["RUSTUP_HOME"]])
        self.assertIn("libnewlib-arm-none-eabi", next(call for call in calls if call[:2] == ["apt-get", "install"]))

    def test_failed_common_check_does_not_publish_environment(self):
        result = self.setup("common", check=False, env=dict(self.env, SETUP_TEST_FAIL_LINK="1"))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.prefix / "env.sh").exists())

    def test_both_boards_preserve_each_others_environment_and_reuse_sdks(self):
        self.board_repositories()
        self.setup("nucleo-f446re")
        self.setup("arduino-nano-33-ble")
        first = self.source_environment()
        self.assertEqual(first[1:], [str(self.prefix / "embedded-sdk" / name) for name in
                                    ("STM32CubeF4-1.28.3", "nrfx-3.2.0", "cmsis-5.9.0")])
        for name in ("nucleo-f446re", "arduino-nano-33-ble"):
            self.assertIn("Using existing SDK", self.setup(name).stdout)
        self.assertEqual(first, self.source_environment())
        self.assertTrue((Path(first[1]) / "Drivers/STM32F4xx_HAL_Driver/Src/stm32f4xx_hal_gpio.c").is_file())

    def test_existing_unmanaged_sdk_is_preserved(self):
        existing = self.prefix / "embedded-sdk/nrfx-3.2.0"
        existing.mkdir(parents=True)
        marker = existing / "user-file"
        marker.write_text("keep")
        result = self.setup("arduino-nano-33-ble", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already exists", result.stdout)
        self.assertEqual(marker.read_text(), "keep")
        self.assertFalse((self.prefix / "env.sh").exists())

    def test_modified_sdk_is_preserved(self):
        self.board_repositories()
        self.setup("arduino-nano-33-ble")
        header = self.prefix / "embedded-sdk/nrfx-3.2.0/mdk/nrf52840.h"
        header.write_text("user change")
        result = self.setup("arduino-nano-33-ble", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("local changes", result.stdout)
        self.assertEqual(header.read_text(), "user change")

    def test_incomplete_submodules_are_reported(self):
        self.board_repositories()
        self.setup("nucleo-f446re")
        sdk = self.prefix / "embedded-sdk/STM32CubeF4-1.28.3"
        self.command("git", "-C", str(sdk), "submodule", "deinit", "-f", "--", "Drivers/STM32F4xx_HAL_Driver")
        result = self.setup("nucleo-f446re", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("submodules are incomplete", result.stdout)

    def test_failed_download_is_cleaned_up_and_can_be_retried(self):
        self.board_repositories()
        result = self.setup("arduino-nano-33-ble", check=False,
                            env=dict(self.env, GIT_ALLOW_PROTOCOL="none"))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.prefix / "env.sh").exists())
        self.assertEqual(list((self.prefix / "embedded-sdk").iterdir()), [])
        self.setup("arduino-nano-33-ble")

    def test_invalid_options_fail_before_any_install(self):
        for args in (("--unknown",), ("--prefix",), ("--prefix", "--dry-run")):
            result = self.setup("common", *args, check=False)
            self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.prefix.exists())

    def test_unsupported_platform_fails_before_any_install(self):
        uname = self.bin / "uname"
        uname.write_text("#!/bin/sh\nprintf 'Darwin\\n'\n")
        uname.chmod(0o755)
        result = self.setup("common", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Ubuntu on Linux or WSL is required", result.stdout)
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.prefix.exists())

    def test_wrong_sdk_revision_is_preserved(self):
        self.board_repositories()
        self.setup("arduino-nano-33-ble")
        sdk = self.prefix / "embedded-sdk/nrfx-3.2.0"
        self.command("git", "-C", str(sdk), "commit", "--allow-empty", "-qm", "user revision")
        revision = self.command("git", "-C", str(sdk), "rev-parse", "HEAD").stdout
        result = self.setup("arduino-nano-33-ble", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("SDK is not at v3.2.0", result.stdout)
        self.assertEqual(self.command("git", "-C", str(sdk), "rev-parse", "HEAD").stdout, revision)


if __name__ == "__main__":
    unittest.main()
