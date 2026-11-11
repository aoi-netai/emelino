//! Project workflows run outside the checkout, including a relocated executable.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_emelino"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn failure(output: Output, message: &str) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create(directory: &Path, board: &str) -> PathBuf {
    success(cli(directory, &["new", "device", "--board", board]));
    directory.join("device")
}

#[test]
fn every_board_template_builds_c_without_python_or_an_sdk() {
    for board in [
        "nucleo-f446re",
        "arduino-nano-33-ble",
        "raspberry-pi-pico",
        "raspberry-pi-pico-2",
        "esp32-c3-devkitm-1-n4x",
    ] {
        let temp = tempfile::Builder::new()
            .prefix("emelino project with spaces '")
            .tempdir()
            .unwrap();
        let project = create(temp.path(), board);
        let output = Command::new(env!("CARGO_BIN_EXE_emelino"))
            .current_dir(&project)
            .args(["build", "--emit", "c"])
            .env("EMELINO_PYTHON", "/missing/python")
            .env_remove("ARM_GCC")
            .env_remove("PICO_SDK_PATH")
            .output()
            .unwrap();
        success(output);
        assert!(project.join("README.md").is_file());
        assert_eq!(
            fs::read_to_string(project.join(".gitignore")).unwrap(),
            "/target/\n/.local/\n"
        );
        let generated = project.join(format!("target/emelino/{board}/generated"));
        let artifact: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(generated.join("emela_artifact.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(artifact["metadata"]["entry"], "emela_embedded_start");
        let expected_export = if board == "esp32-c3-devkitm-1-n4x" {
            let readme = fs::read_to_string(project.join("README.md")).unwrap();
            assert!(readme.contains("external LED") && readme.contains("RISCV32_ESP_GCC"));
            "emelino_app_main"
        } else {
            "app_main"
        };
        assert_eq!(
            artifact["metadata"]["exports"][0]["c_symbol"],
            expected_export
        );
        assert!(generated.join("emela_embedded.c").is_file());
        // Saved inputs remain consumable through the explicit compiler CLI.
        success(cli(
            &generated,
            &[
                "build",
                "--input",
                "program.backend.json",
                "--config",
                "program.config.json",
                "--out-dir",
                "replay",
            ],
        ));
        assert_eq!(
            fs::read(generated.join("emela_embedded.c")).unwrap(),
            fs::read(generated.join("replay/emela_embedded.c")).unwrap()
        );
    }
}

#[test]
fn new_validates_before_writing_and_preserves_existing_directories() {
    let temp = tempfile::tempdir().unwrap();
    failure(
        cli(temp.path(), &["new", "device", "--board", "../bad"]),
        "unknown board",
    );
    assert!(!temp.path().join("device").exists());
    failure(cli(temp.path(), &["new", "device"]), "--board is required");
    failure(
        cli(
            temp.path(),
            &[
                "new",
                "device",
                "--board",
                "nucleo-f446re",
                "--board",
                "nucleo-f446re",
            ],
        ),
        "more than once",
    );
    failure(
        cli(
            temp.path(),
            &["new", "bad name", "--board", "nucleo-f446re"],
        ),
        "invalid project name",
    );
    let project = create(temp.path(), "nucleo-f446re");
    fs::write(project.join("keep.txt"), "keep").unwrap();
    let before = fs::read(project.join("Emelino.toml")).unwrap();
    failure(
        cli(
            temp.path(),
            &["new", "device", "--board", "raspberry-pi-pico"],
        ),
        "already exists",
    );
    assert_eq!(before, fs::read(project.join("Emelino.toml")).unwrap());
    assert_eq!(
        fs::read_to_string(project.join("keep.txt")).unwrap(),
        "keep"
    );
    success(cli(
        temp.path(),
        &[
            "new",
            "directory with spaces",
            "--name",
            "another-device",
            "--board",
            "nucleo-f446re",
        ],
    ));
}

#[test]
fn project_build_finds_parents_collects_modules_and_executes_generated_c() {
    let temp = tempfile::tempdir().unwrap();
    let project = create(temp.path(), "nucleo-f446re");
    fs::write(
        project.join("src/main.emel"),
        "import Math\npub fn main() -> Int { Math.answer() }\n",
    )
    .unwrap();
    fs::write(
        project.join("src/math.emel"),
        "pub fn answer() -> Int { 42 }\n",
    )
    .unwrap();
    fs::create_dir(project.join("src/nested")).unwrap();
    fs::write(
        project.join("src/nested/extra.emel"),
        "pub fn value() -> Int { 7 }\n",
    )
    .unwrap();
    success(cli(&project.join("src/nested"), &["build", "--emit", "c"]));
    let generated = project.join("target/emelino/nucleo-f446re/generated");
    fs::write(
        project.join("host.c"),
        r#"
#include "emela_embedded.h"
#include "emela_board.h"
#include <stdlib.h>
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
    (void)k; (void)m; (void)n; abort();
}
void emela_platform_start(void (*program)(void)) { program(); }
int main(void) { emela_embedded_start(); return app_main() == 42 ? 0 : 1; }
"#,
    )
    .unwrap();
    let host = project.join("host");
    success(
        Command::new("cc")
            .current_dir(&project)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-O0", "-I"])
            .arg(&generated)
            .arg(generated.join("emela_embedded.c"))
            .args(["host.c", "-o"])
            .arg(&host)
            .output()
            .unwrap(),
    );
    success(Command::new(host).output().unwrap());
    // An entry nested under src/ still imports from the same source root.
    fs::rename(
        project.join("src/main.emel"),
        project.join("src/nested/main.emel"),
    )
    .unwrap();
    let manifest = fs::read_to_string(project.join("Emelino.toml"))
        .unwrap()
        .replace("src/main.emel", "src/nested/main.emel");
    fs::write(project.join("Emelino.toml"), manifest).unwrap();
    success(cli(temp.path(), &["build", "device", "--emit", "c"]));
}

#[test]
fn malformed_settings_fail_with_actionable_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    failure(cli(temp.path(), &["build"]), "emelino new");
    let project = create(temp.path(), "nucleo-f446re");
    let path = project.join("Emelino.toml");
    let manifest = fs::read_to_string(&path).unwrap();
    for (text, message) in [
        ("[project".into(), "invalid"),
        (
            manifest.replace("heap_bytes", "heap_bytez"),
            "unknown field",
        ),
        (
            manifest.replace("src/main.emel", "../main.emel"),
            "relative path",
        ),
        (
            manifest.replace("src/main.emel", "other/main.emel"),
            "under src/",
        ),
        (
            manifest.replace("src/main.emel", "src/missing.emel"),
            "entry is missing",
        ),
        (
            manifest.replace("nucleo-f446re", "unknown-board"),
            "unknown board",
        ),
        (
            format!("{manifest}\n[firmware]\noptimization = 'O9'\n"),
            "optimization",
        ),
        (
            format!("{manifest}\n[firmware]\nstack_bytes = 1\n"),
            "multiples of 8",
        ),
        (
            format!("{manifest}\n[firmware]\nsources = ['../outside.c']\n"),
            "relative path",
        ),
    ] {
        fs::write(&path, text).unwrap();
        failure(cli(&project, &["build", "--emit", "c"]), message);
    }
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_outputs_or_create_source_loops() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let project = create(temp.path(), "nucleo-f446re");
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, project.join("target")).unwrap();
    failure(cli(&project, &["build", "--emit", "c"]), "symlink");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_file(project.join("target")).unwrap();
    symlink(project.join("src"), project.join("src/loop")).unwrap();
    failure(cli(&project, &["build", "--emit", "c"]), "symlinks");
    fs::remove_file(project.join("src/loop")).unwrap();
    success(cli(&project, &["build", "--emit", "c"]));
    let generated = project.join("target/emelino/nucleo-f446re/generated/emela_embedded.c");
    fs::remove_file(&generated).unwrap();
    let sentinel = outside.join("keep.c");
    fs::write(&sentinel, "keep").unwrap();
    symlink(&sentinel, &generated).unwrap();
    failure(cli(&project, &["build", "--emit", "c"]), "symlinks");
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
}

#[cfg(unix)]
#[test]
fn relocated_binary_bundles_firmware_and_forwards_settings_without_a_shell() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::Builder::new()
        .prefix("emelino relocated '")
        .tempdir()
        .unwrap();
    let binary = temp.path().join("emelino");
    fs::copy(env!("CARGO_BIN_EXE_emelino"), &binary).unwrap();
    success(
        Command::new(&binary)
            .current_dir(temp.path())
            .args(["new", "device", "--board", "raspberry-pi-pico-2"])
            .output()
            .unwrap(),
    );
    let project = temp.path().join("device");
    let manifest = project.join("Emelino.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{text}\n[firmware]\noptimization = 'O0'\nstack_bytes = 8192\nguard_bytes = 64\nsources = ['native extra.c']\nlink_args = ['-Wl,--defsym=example=1']\n")).unwrap();
    fs::write(project.join("native extra.c"), "/* native source */\n").unwrap();
    let python = temp.path().join("python stub");
    fs::write(
        &python,
        r#"#!/usr/bin/python3
import json
from pathlib import Path
import sys
assert sys.argv[1] == '-B'
script = Path(sys.argv[2])
support = script.parent.parent
sys.path.insert(0, str(support))
from firmware.build.profiles import load_profiles
args = sys.argv[3:]
board = args[args.index('--board') + 1]
profile, chip, directory = load_profiles(board)
assert profile['port'] == 'pico_sdk' and chip['id'] == 'rp2350a'
assert (support / 'firmware/ports/pico_sdk/CMakeLists.txt').is_file()
assert (support / 'tools/setup/pico-sdk.sh').is_file()
assert args[args.index('--optimization') + 1] == 'O0'
assert args[args.index('--stack-bytes') + 1] == '8192'
assert args[args.index('--guard-bytes') + 1] == '64'
assert Path(args[args.index('--source') + 1]).read_text().startswith('/* native')
assert '--link-arg=-Wl,--defsym=example=1' in args
assert Path(args[args.index('--sdk-dir') + 1]).is_absolute()
assert Path(args[args.index('--cc') + 1]).is_absolute()
output = Path(args[args.index('--output-dir') + 1])
(output / 'forwarded.json').write_text(json.dumps(args))
(output / 'firmware.json').write_text('{}')
"#,
    )
    .unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    success(
        Command::new(&binary)
            .current_dir(temp.path())
            .args([
                "build",
                "device",
                "--sdk-dir",
                "SDK with spaces",
                "--cc",
                "./toolchain/gcc",
            ])
            .env("EMELINO_PYTHON", &python)
            .output()
            .unwrap(),
    );
    let firmware = project.join("target/emelino/raspberry-pi-pico-2/firmware");
    let args: Vec<String> =
        serde_json::from_str(&fs::read_to_string(firmware.join("forwarded.json")).unwrap())
            .unwrap();
    assert!(args.contains(&temp.path().join("SDK with spaces").display().to_string()));
    assert!(firmware.join("firmware.json").exists());
    fs::write(&python, "#!/usr/bin/python3\nimport sys\nsys.exit(23)\n").unwrap();
    failure(
        Command::new(&binary)
            .current_dir(&project)
            .arg("build")
            .env("EMELINO_PYTHON", &python)
            .output()
            .unwrap(),
        "firmware build failed",
    );
    assert!(!firmware.join("firmware.json").exists());
    fs::write(&python, "#!/usr/bin/python3\n").unwrap();
    failure(
        Command::new(&binary)
            .current_dir(&project)
            .arg("build")
            .env("EMELINO_PYTHON", &python)
            .output()
            .unwrap(),
        "without publishing firmware.json",
    );
    fs::write(firmware.join("firmware.json"), "{}").unwrap();
    // A compiler failure must invalidate the previous firmware result.
    fs::write(
        project.join("src/main.emel"),
        "pub fn main() -> Int { broken syntax",
    )
    .unwrap();
    failure(
        Command::new(&binary)
            .current_dir(&project)
            .arg("build")
            .env("EMELINO_PYTHON", &python)
            .output()
            .unwrap(),
        "parsing",
    );
    assert!(!firmware.join("firmware.json").exists());
}

#[test]
fn missing_python_explains_setup_and_does_not_preserve_success() {
    let temp = tempfile::tempdir().unwrap();
    let project = create(temp.path(), "nucleo-f446re");
    let output = Command::new(env!("CARGO_BIN_EXE_emelino"))
        .current_dir(&project)
        .arg("build")
        .env("EMELINO_PYTHON", "/missing/python")
        .output()
        .unwrap();
    failure(output, "tools/setup/nucleo-f446re.sh");
    assert!(
        !project
            .join("target/emelino/nucleo-f446re/firmware/firmware.json")
            .exists()
    );
    success(cli(temp.path(), &["boards"]));
    success(cli(temp.path(), &["new", "--help"]));
    success(cli(temp.path(), &["build", "--help"]));
}

#[test]
fn bundled_real_python_builder_loads_each_port_and_reports_missing_dependencies() {
    for (board, expected) in [
        ("nucleo-f446re", "ARM GCC not found"),
        ("arduino-nano-33-ble", "Nordic nrfx MDK required"),
        ("raspberry-pi-pico", "ARM GCC not found"),
        ("raspberry-pi-pico-2", "ARM GCC not found"),
        ("esp32-c3-devkitm-1-n4x", "riscv32-esp-elf-gcc required"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let project = create(temp.path(), board);
        let output = Command::new(env!("CARGO_BIN_EXE_emelino"))
            .current_dir(&project)
            .args([
                "build",
                "--cc",
                "/missing/arm-none-eabi-gcc",
                "--sdk-dir",
                "/missing/sdk",
            ])
            .env_remove("EMELINO_PYTHON")
            .env_remove("NRFX_ROOT")
            .env_remove("CMSIS_ROOT")
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Traceback"));
        failure(output, expected);
        assert!(
            !project
                .join(format!("target/emelino/{board}/firmware/firmware.json"))
                .exists()
        );
    }
}
