use crate::{
    firmware,
    project::{self, Project},
};
use emelino_cli::{errors, frontend, upstream_adapter};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{fs, io};

pub const HELP: &str = "Usage: emelino build [directory] [--emit c|firmware] [--cc <executable>] [--sdk-dir <directory>]\n\nFinds Emelino.toml in the directory or its parents. Defaults to firmware.\n--emit c writes C and replay inputs without Python, a cross compiler or an SDK.\nRelative --cc and --sdk-dir paths are resolved from the calling directory.";

pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut args = args;
    let mut directory = None;
    let mut emit = None;
    let mut cc = None;
    let mut sdk = None;
    while let Some(arg) = args.next() {
        if arg == "--emit" || arg == "--cc" || arg == "--sdk-dir" {
            let value = args
                .next()
                .filter(|value| !value.is_empty() && !value.to_string_lossy().starts_with('-'))
                .ok_or_else(|| format!("missing value for {}", arg.to_string_lossy()))?;
            let target = if arg == "--emit" {
                &mut emit
            } else if arg == "--cc" {
                &mut cc
            } else {
                &mut sdk
            };
            if target.replace(value).is_some() {
                return Err(format!(
                    "{} was specified more than once",
                    arg.to_string_lossy()
                ));
            }
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}\n{HELP}", arg.to_string_lossy()));
        } else if directory.replace(PathBuf::from(arg)).is_some() {
            return Err(format!("expected at most one project directory\n{HELP}"));
        }
    }
    let emit = emit.unwrap_or_else(|| "firmware".into());
    if emit != "c" && emit != "firmware" {
        return Err("--emit must be c or firmware".into());
    }
    if emit == "c" && (cc.is_some() || sdk.is_some()) {
        return Err("--cc and --sdk-dir require --emit firmware".into());
    }
    let project = Project::discover(directory.as_deref().unwrap_or(Path::new(".")))?;
    let output = project::directory(
        &project.root,
        &Path::new("target/emelino").join(&project.manifest.project.board),
    )?;
    let generated = project::directory(&output, Path::new("generated"))?;
    let firmware_dir = project::directory(&output, Path::new("firmware"))?;
    project::reject_output_symlinks(&output)?;
    // A previous firmware success must not survive a failed source compilation.
    match fs::remove_file(firmware_dir.join("firmware.json")) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("cannot invalidate previous firmware success: {e}")),
    }
    let sources = project.sources()?;
    let supplied: Vec<_> = sources
        .iter()
        .map(|(name, text)| frontend::Source { name, text })
        .collect();
    let mut config = project.manifest.resources.config();
    if crate::firmware::board(&project.manifest.project.board)?.port == "esp_idf" {
        // ESP-IDF owns void app_main(void); the Emela entry's C export must
        // have a different name and may return an Int or another Emela type.
        config.exports[0].symbol = "emelino_app_main".into();
    }
    let entry = project.root.join(&project.manifest.project.entry);
    let flow = frontend::generate_sources(&supplied, &project.entry_name()?, &entry)?;
    let program = upstream_adapter::adapt(&flow.module, &config.export_pairs()?)?;
    config.validate_entry(&program)?;
    let input = emelino_compiler::BackendInput::new(program).map_err(errors)?;
    // Stage all files before publishing, and reject redirected generated files.
    let staging = tempfile::Builder::new()
        .prefix(".generated-")
        .tempdir_in(&output)
        .map_err(|e| e.to_string())?;
    super::compile::save(input, Some(flow.dump), &config, staging.path(), true)?;
    for file in fs::read_dir(staging.path()).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        let destination = generated.join(file.file_name());
        if let Ok(meta) = fs::symlink_metadata(&destination)
            && (!meta.is_file() || meta.is_symlink())
        {
            return Err(format!(
                "generated output must not be a symlink or directory: {}",
                destination.display()
            ));
        }
    }
    for file in fs::read_dir(staging.path()).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        fs::rename(file.path(), generated.join(file.file_name())).map_err(|e| e.to_string())?;
    }
    println!(
        "Generated C for {} in {}",
        project.manifest.project.name,
        generated.display()
    );
    if emit == "firmware" {
        let caller = std::env::current_dir().map_err(|e| e.to_string())?;
        let cc = cc.map(PathBuf::from).map(|path| {
            if path.components().count() > 1 {
                caller.join(path)
            } else {
                path
            }
        });
        let sdk = sdk.map(|path| caller.join(path));
        firmware::build(
            &project,
            &generated,
            &firmware_dir,
            cc.as_deref(),
            sdk.as_deref(),
        )?;
    }
    Ok(())
}
