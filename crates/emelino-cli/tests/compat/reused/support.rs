//! Reused source observations through the real upstream frontend and C backend.
//! Fixtures retain the old C harness and resource limits. Public C aliases moved
//! to configuration; documented syntax and external-handler adjustments keep
//! source fixtures valid for the pinned upstream frontend.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "emelino-reused-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed fixture at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn success(context: &str, output: Output) {
    assert!(
        output.status.success(),
        "{context}: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cli(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_emelino"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run emelino")
}

fn generated_c_files(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "c" || extension == "h")
        {
            files.insert(
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            );
        }
    }
    assert!(files.contains_key("emela_embedded.c"));
    assert!(files.contains_key("emela_embedded.h"));
    files
}

pub fn run_fixture(name: &str, heap_observer: bool) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compat/reused")
        .join(name);
    run_fixture_at(name, &fixture, heap_observer);
}

pub fn run_fixture_at(name: &str, fixture: &Path, heap_observer: bool) {
    let scratch = Scratch::new(name);
    fs::copy(fixture.join("main.emel"), scratch.0.join("main.emel")).unwrap();
    fs::copy(fixture.join("config.json"), scratch.0.join("config.json")).unwrap();
    fs::copy(fixture.join("harness.c"), scratch.0.join("harness.c")).unwrap();
    success(
        name,
        cli(
            &scratch.0,
            &[
                "build",
                "main.emel",
                "--config",
                "config.json",
                "--out-dir",
                "source",
            ],
        ),
    );
    let from_source = generated_c_files(&scratch.0.join("source"));
    let mut options: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(scratch.0.join("config.json")).unwrap()).unwrap();
    options.as_object_mut().unwrap().remove("exports");
    options.as_object_mut().unwrap().remove("sources");
    fs::write(
        scratch.0.join("options.json"),
        serde_json::to_vec_pretty(&options).unwrap(),
    )
    .unwrap();
    fs::remove_file(scratch.0.join("main.emel")).unwrap();
    fs::remove_file(scratch.0.join("config.json")).unwrap();
    success(
        &format!("{name} saved input replay"),
        cli(
            &scratch.0,
            &[
                "build",
                "--input",
                "source/program.backend.json",
                "--config",
                "options.json",
                "--out-dir",
                "replay",
            ],
        ),
    );
    let from_saved = generated_c_files(&scratch.0.join("replay"));
    assert_eq!(from_source, from_saved, "{name}: saved input changed C");

    if heap_observer {
        let path = scratch.0.join("replay/emela_embedded.c");
        let mut source = fs::read_to_string(&path).unwrap();
        assert!(source.contains("emela_heap_storage"));
        assert!(!source.contains("malloc(") && !source.contains("calloc("));
        source.push_str(include_str!("heap-observer.c"));
        fs::write(path, source).unwrap();
    }
    let translation_units: Vec<_> = from_saved
        .keys()
        .filter(|name| name.ends_with(".c"))
        .map(|name| scratch.0.join("replay").join(name))
        .collect();
    for optimization in ["-O0", "-O2"] {
        let executable = scratch.0.join(format!("probe{}", &optimization[1..]));
        success(
            &format!("{name} C compilation {optimization}"),
            Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .args(["-std=c11", "-Wall", "-Werror", optimization, "-I"])
                .arg(scratch.0.join("replay"))
                .args(&translation_units)
                .arg(scratch.0.join("harness.c"))
                .args(["-lm", "-o"])
                .arg(&executable)
                .output()
                .expect("host C compiler is required for compatibility tests"),
        );
        success(
            &format!("{name} C execution {optimization}"),
            Command::new(&executable).output().unwrap(),
        );
    }
}
