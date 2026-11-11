use crate::{firmware, project, scaffold};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

pub const HELP: &str = "Usage: emelino new <directory> --board <board> [--name <name>]\n\nCreates Emelino.toml, src/main.emel, README.md and .gitignore.\nThe destination must not exist. No SDK or Python installation is required.";

pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut args = args;
    let mut destination = None;
    let mut board = None;
    let mut name = None;
    while let Some(arg) = args.next() {
        if arg == "--board" || arg == "--name" {
            let value = args
                .next()
                .filter(|value| !value.is_empty() && !value.to_string_lossy().starts_with('-'))
                .ok_or_else(|| format!("missing value for {}", arg.to_string_lossy()))?;
            let value = value
                .into_string()
                .map_err(|_| "board and name must be UTF-8")?;
            let target = if arg == "--board" {
                &mut board
            } else {
                &mut name
            };
            if target.replace(value).is_some() {
                return Err(format!(
                    "{} was specified more than once",
                    arg.to_string_lossy()
                ));
            }
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}\n{HELP}", arg.to_string_lossy()));
        } else if destination.replace(PathBuf::from(arg)).is_some() {
            return Err(format!("expected exactly one directory\n{HELP}"));
        }
    }
    let destination = destination.ok_or_else(|| format!("missing directory\n{HELP}"))?;
    let board = firmware::board(&board.ok_or_else(|| format!("--board is required\n{HELP}"))?)?;
    let name = name
        .or_else(|| {
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .ok_or("cannot infer project name; pass --name")?;
    if !project::valid_name(&name) {
        return Err("invalid project name; pass --name with a name starting with a letter and containing only letters, digits, `_` or `-`".into());
    }
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "destination already exists: {}",
            destination.display()
        ));
    }
    let absolute = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(&destination);
    let parent = absolute
        .parent()
        .ok_or("destination must have a parent directory")?;
    fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(".emelino-new-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    fs::create_dir(staging.path().join("src")).map_err(|e| e.to_string())?;
    for (path, content) in [
        ("Emelino.toml", scaffold::manifest(&name, &board)),
        ("src/main.emel", scaffold::source(&board)),
        ("README.md", scaffold::readme(&name, &board)),
        (".gitignore", "/target/\n/.local/\n".into()),
    ] {
        fs::write(staging.path().join(path), content)
            .map_err(|e| format!("cannot create {path}: {e}"))?;
    }
    fs::rename(staging.path(), &absolute)
        .map_err(|e| format!("cannot create {}: {e}", destination.display()))?;
    println!(
        "Created {name} for {} in {}\nNext: cd into the project and run `emelino build`.",
        board.physical_board,
        destination.display()
    );
    Ok(())
}
