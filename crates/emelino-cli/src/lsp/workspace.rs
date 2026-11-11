use super::text;
use crate::project::{self, Project};
use emela_resolve::ModuleId;
use emelino_cli::analysis::{self, Analysis, Source};
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct CheckedProject {
    pub project: Project,
    pub analysis: Analysis,
    library: PathBuf,
}

impl CheckedProject {
    pub fn check(project: Project, overlay: &BTreeMap<PathBuf, String>) -> Result<Self, String> {
        let sources = project.sources_with(overlay)?;
        let supplied: Vec<_> = sources
            .iter()
            .map(|(name, text)| Source { name, text })
            .collect();
        let analysis = analysis::analyze_sources(&supplied, &project.entry_name()?)?;
        let library = library_sources(&project.root)?;
        Ok(Self {
            project,
            analysis,
            library,
        })
    }

    pub fn module_path(&self, module: ModuleId) -> Option<PathBuf> {
        if self.analysis.modules.is_core(module) {
            return None;
        }
        let relative = self.analysis.modules[module]
            .file
            .strip_prefix("<emelino-input>")
            .ok()?;
        if emelino_cli::embedded_sources()
            .iter()
            .any(|(name, _)| Path::new(name) == relative)
        {
            Some(self.library.join(relative))
        } else {
            Some(self.project.root.join("src").join(relative))
        }
    }

    pub fn module_at(&self, path: &Path) -> Option<ModuleId> {
        self.analysis
            .modules
            .iter()
            .find_map(|(id, _)| (self.module_path(id).as_deref() == Some(path)).then_some(id))
    }
}

pub fn discover(path: &Path, overlay: &BTreeMap<PathBuf, String>) -> Result<Project, String> {
    let directory = if path.is_dir() {
        path
    } else {
        path.parent().ok_or("file has no parent")?
    };
    let directory = directory
        .ancestors()
        .find(|p| p.is_dir())
        .ok_or("project directory is missing")?;
    Project::discover_with(directory, overlay)
}

pub fn anchor(path: &Path, overlay: &BTreeMap<PathBuf, String>) -> PathBuf {
    path.ancestors()
        .map(|p| p.join(project::MANIFEST))
        .find(|p| p.is_file() || overlay.contains_key(p))
        .unwrap_or_else(|| path.to_owned())
}

fn library_sources(root: &Path) -> Result<PathBuf, String> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    emelino_cli::embedded_sources().hash(&mut hash);
    let library = project::directory(
        root,
        &Path::new("target/emelino/lsp").join(format!("library-{:016x}", hash.finish())),
    )?;
    for (name, contents) in emelino_cli::embedded_sources() {
        let parent = project::directory(&library, Path::new(name).parent().unwrap())?;
        let path = library.join(name);
        if let Ok(meta) = fs::symlink_metadata(&path)
            && (!meta.is_file() || meta.is_symlink())
        {
            return Err(format!(
                "library file must not be a symlink: {}",
                path.display()
            ));
        }
        if fs::read_to_string(&path).ok().as_deref() != Some(*contents) {
            let mut temporary =
                tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            temporary
                .write_all(contents.as_bytes())
                .map_err(|e| e.to_string())?;
            let mut permissions = temporary
                .as_file()
                .metadata()
                .map_err(|e| e.to_string())?
                .permissions();
            permissions.set_readonly(true);
            temporary
                .as_file()
                .set_permissions(permissions)
                .map_err(|e| e.to_string())?;
            temporary.persist(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(text::normalize(&library))
}
