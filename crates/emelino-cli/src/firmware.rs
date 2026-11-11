//! Version-matched firmware resources are carried by the executable.
use crate::project::{self, Project};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

mod bundled {
    include!(concat!(env!("OUT_DIR"), "/support.rs"));
}

#[derive(Deserialize)]
pub struct Board {
    pub board: String,
    pub physical_board: String,
    pub port: String,
    pub pins: BTreeMap<String, u16>,
}

pub fn boards() -> Vec<Board> {
    bundled::FILES
        .iter()
        .filter(|(name, _)| name.starts_with("firmware/boards/") && name.ends_with("/board.json"))
        .map(|(_, data)| serde_json::from_slice(data).expect("bundled board profile"))
        .collect()
}

pub fn board(name: &str) -> Result<Board, String> {
    boards()
        .into_iter()
        .find(|board| board.board == name)
        .ok_or_else(|| {
            format!(
                "unknown board `{name}`; supported boards: {}",
                boards()
                    .into_iter()
                    .map(|board| board.board)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

pub fn unpack(root: &Path) -> Result<PathBuf, String> {
    let destination = project::directory(
        root,
        &Path::new("target/emelino").join(format!("support-{}", bundled::ID)),
    )?;
    for (name, contents) in bundled::FILES {
        let path = destination.join(name);
        let parent = Path::new(name).parent().unwrap();
        project::directory(&destination, parent)?;
        if let Ok(meta) = fs::symlink_metadata(&path)
            && (!meta.is_file() || meta.is_symlink())
        {
            return Err(format!(
                "bundled support file must not be a symlink: {}",
                path.display()
            ));
        }
        if fs::read(&path).ok().as_deref() != Some(*contents) {
            fs::write(&path, contents)
                .map_err(|e| format!("cannot unpack {}: {e}", path.display()))?;
        }
    }
    Ok(destination)
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

pub fn setup_hint(project: &Project, support: &Path) -> String {
    let script = match board(&project.manifest.project.board)
        .unwrap()
        .port
        .as_str()
    {
        "stm32f4" => "nucleo-f446re",
        "nrf52" => "arduino-nano-33-ble",
        "pico_sdk" => "pico-sdk",
        _ => unreachable!(),
    };
    let prefix = project.root.join(".local");
    format!(
        "Install tools and SDK on Ubuntu / WSL, then load the environment:\n  bash {} --prefix {}\n  bash {} --prefix {}\n  source {}\nC generation alone needs no SDK: emelino build --emit c",
        quote(&support.join("tools/setup/common.sh")),
        quote(&prefix),
        quote(&support.join(format!("tools/setup/{script}.sh"))),
        quote(&prefix),
        quote(&prefix.join("env.sh"))
    )
}

pub fn build(
    project: &Project,
    generated: &Path,
    output: &Path,
    cc: Option<&Path>,
    sdk: Option<&Path>,
) -> Result<(), String> {
    let support = unpack(&project.root)?;
    let python = env::var_os("EMELINO_PYTHON").unwrap_or_else(|| "python3".into());
    let firmware = &project.manifest.firmware;
    let mut command = Command::new(python);
    command
        .arg("-B")
        .arg(support.join("tools/build_firmware.py"))
        .arg("--board")
        .arg(&project.manifest.project.board)
        .arg("--generated-dir")
        .arg(generated)
        .arg("--output-dir")
        .arg(output)
        .arg("--optimization")
        .arg(&firmware.optimization)
        .arg("--stack-bytes")
        .arg(firmware.stack_bytes.to_string())
        .arg("--guard-bytes")
        .arg(firmware.guard_bytes.to_string())
        .current_dir(&project.root);
    if let Some(cc) = cc {
        command.arg("--cc").arg(cc);
    }
    if let Some(sdk) = sdk {
        command.arg("--sdk-dir").arg(sdk);
    }
    for source in &firmware.sources {
        command.arg("--source").arg(project.root.join(source));
    }
    for arg in &firmware.link_args {
        command.arg(format!("--link-arg={arg}"));
    }
    let status = command.status().map_err(|e| {
        format!(
            "cannot run Python firmware builder: {e}; install Python 3 or set EMELINO_PYTHON\n{}",
            setup_hint(project, &support)
        )
    })?;
    if !status.success() {
        return Err(format!(
            "firmware build failed ({status})\n{}",
            setup_hint(project, &support)
        ));
    }
    if !output.join("firmware.json").is_file() {
        return Err("firmware builder exited without publishing firmware.json".into());
    }
    println!(
        "Built firmware for {} in {}",
        project.manifest.project.board,
        output.display()
    );
    Ok(())
}
