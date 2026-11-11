//! Project discovery and filesystem inputs stay outside the compiler.
use emelino_cli::config::{Config, Export};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::{fs, io};

pub const MANIFEST: &str = "Emelino.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub project: Settings,
    #[serde(default)]
    pub resources: Resources,
    #[serde(default)]
    pub firmware: Firmware,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub name: String,
    pub board: String,
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "src/main.emel".into()
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Resources {
    pub heap_bytes: Option<u32>,
    pub string_capacity: Option<u16>,
    pub peripheral_buffer_capacity: Option<u16>,
    pub const_diagnostic_bytes: Option<u16>,
    pub fiber_capacity: Option<u16>,
    pub frame_capacity: Option<u16>,
    pub frame_bytes: Option<u16>,
    pub task_capacity: Option<u16>,
    pub event_capacity: Option<u16>,
    pub deferred_capacity: Option<u16>,
}

impl Resources {
    pub fn config(&self) -> Config {
        Config {
            entry: Some("main".into()),
            exports: vec![Export {
                function: "main".into(),
                symbol: "app_main".into(),
            }],
            heap_bytes: self.heap_bytes,
            string_capacity: self.string_capacity,
            peripheral_buffer_capacity: self.peripheral_buffer_capacity,
            const_diagnostic_bytes: self.const_diagnostic_bytes,
            fiber_capacity: self.fiber_capacity,
            frame_capacity: self.frame_capacity,
            frame_bytes: self.frame_bytes,
            task_capacity: self.task_capacity,
            event_capacity: self.event_capacity,
            deferred_capacity: self.deferred_capacity,
            ..Config::default()
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Firmware {
    pub optimization: String,
    pub stack_bytes: u32,
    pub guard_bytes: u32,
    pub sources: Vec<String>,
    pub link_args: Vec<String>,
}

impl Default for Firmware {
    fn default() -> Self {
        Self {
            optimization: "Os".into(),
            stack_bytes: 16384,
            guard_bytes: 256,
            sources: Vec::new(),
            link_args: Vec::new(),
        }
    }
}

pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
}

pub fn valid_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn relative(path: &str) -> Result<&Path, String> {
    let value = Path::new(path);
    if path.is_empty()
        || value
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("expected a relative path without `..`: {path}"));
    }
    Ok(value)
}

impl Project {
    pub fn discover(directory: &Path) -> Result<Self, String> {
        Self::discover_with(directory, &BTreeMap::new())
    }

    pub fn discover_with(
        directory: &Path,
        overlay: &BTreeMap<PathBuf, String>,
    ) -> Result<Self, String> {
        let directory = directory
            .canonicalize()
            .map_err(|e| format!("cannot open project directory {}: {e}", directory.display()))?;
        if !directory.is_dir() {
            return Err(format!(
                "expected a project directory: {}",
                directory.display()
            ));
        }
        let root = directory.ancestors().find(|root| root.join(MANIFEST).exists() || overlay.contains_key(&root.join(MANIFEST)))
            .ok_or_else(|| format!("no {MANIFEST} found; create a project with `emelino new <directory> --board <board>`"))?;
        let path = root.join(MANIFEST);
        let text = overlay
            .get(&path)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| fs::read_to_string(&path))
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let manifest: Manifest =
            toml::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?;
        if !valid_name(&manifest.project.name) {
            return Err("project.name must start with a letter and contain only letters, digits, `_` or `-`".into());
        }
        crate::firmware::board(&manifest.project.board)?;
        let entry = relative(&manifest.project.entry)?;
        if !entry.starts_with("src") || entry.extension().is_none_or(|e| e != "emel") {
            return Err("project.entry must be an .emel file under src/".into());
        }
        let firmware = &manifest.firmware;
        if !matches!(firmware.optimization.as_str(), "O0" | "O1" | "O2" | "Os") {
            return Err("firmware.optimization must be O0, O1, O2 or Os".into());
        }
        if firmware.stack_bytes == 0
            || !firmware.stack_bytes.is_multiple_of(8)
            || firmware.guard_bytes < 32
            || !firmware.guard_bytes.is_multiple_of(8)
        {
            return Err("firmware stack must be positive and guard at least 32; both must be multiples of 8".into());
        }
        for source in &firmware.sources {
            relative(source)?;
        }
        Ok(Self {
            root: root.to_owned(),
            manifest,
        })
    }

    /// Names are relative to src/, even when the selected entry is nested.
    pub fn sources(&self) -> Result<Vec<(String, String)>, String> {
        self.sources_with(&BTreeMap::new())
    }

    pub fn sources_with(
        &self,
        overlay: &BTreeMap<PathBuf, String>,
    ) -> Result<Vec<(String, String)>, String> {
        let root = self.root.join("src");
        let mut files = Vec::new();
        collect_sources(&root, &root, &mut files, overlay)?;
        for (path, text) in overlay {
            if let Ok(name) = path.strip_prefix(&root)
                && name.extension().is_some_and(|ext| ext == "emel")
            {
                let name = name
                    .to_str()
                    .ok_or("source paths must be UTF-8")?
                    .replace('\\', "/");
                relative(&name)?;
                if !files.iter().any(|(existing, _)| existing == &name) {
                    files.push((name, text.clone()));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let entry = self.entry_name()?;
        if !files.iter().any(|(name, _)| name == &entry) {
            return Err(format!(
                "project entry is missing: {}",
                self.manifest.project.entry
            ));
        }
        Ok(files)
    }

    pub fn entry_name(&self) -> Result<String, String> {
        Ok(relative(&self.manifest.project.entry)?
            .strip_prefix("src")
            .unwrap()
            .to_str()
            .ok_or("entry path must be UTF-8")?
            .replace('\\', "/"))
    }
}

fn collect_sources(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, String)>,
    overlay: &BTreeMap<PathBuf, String>,
) -> Result<(), String> {
    let meta = fs::symlink_metadata(directory)
        .map_err(|e| format!("cannot read source directory {}: {e}", directory.display()))?;
    if !meta.is_dir() || meta.is_symlink() {
        return Err(format!(
            "source directories must be real directories: {}",
            directory.display()
        ));
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "symlinks are not supported under src/: {}",
                path.display()
            ));
        }
        if kind.is_dir() {
            collect_sources(root, &path, files, overlay)?;
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "emel") {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .ok_or("source paths must be UTF-8")?
                .replace('\\', "/");
            let text = overlay
                .get(&path)
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| fs::read_to_string(&path))
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            files.push((name, text));
        }
    }
    Ok(())
}

/// Never follow an output-directory symlink into user sources or SDKs.
pub fn directory(root: &Path, relative_path: &Path) -> Result<PathBuf, String> {
    let mut path = root.to_owned();
    for part in relative_path.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err(format!("invalid output path: {}", relative_path.display()));
        }
        path.push(part);
        match fs::create_dir(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !meta.is_dir() || meta.is_symlink() {
                    return Err(format!(
                        "output directory must not be a symlink or file: {}",
                        path.display()
                    ));
                }
            }
            Err(e) => return Err(format!("cannot create {}: {e}", path.display())),
        }
    }
    Ok(path)
}

pub fn reject_output_symlinks(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
            return Err(format!(
                "output must not contain symlinks or special files: {}",
                entry.path().display()
            ));
        }
        if kind.is_dir() {
            reject_output_symlinks(&entry.path())?;
        }
    }
    Ok(())
}
