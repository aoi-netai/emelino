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

    check_artifact_observations(fixture, &scratch.0, &options);
    apply_observation_seams(fixture, &scratch.0);
    if heap_observer {
        let path = scratch.0.join("replay/emela_embedded.c");
        let mut source = fs::read_to_string(&path).unwrap();
        assert!(!source.contains("malloc(") && !source.contains("calloc("));
        if source.contains("emela_heap_storage") {
            source.push_str(include_str!("heap-observer.c"));
        } else {
            let observations: serde_json::Value =
                serde_json::from_slice(&fs::read(fixture.join("observations.json")).unwrap())
                    .unwrap();
            assert_eq!(observations["heapless_observer"], true);
            source.push_str("\nint test_heap_empty(void) { return 1; }\n");
        }
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

// These are existing test-only public-header lookups and generated-C seams.
// Metadata names the exact original observation; no production input is changed.
fn apply_observation_seams(fixture: &Path, directory: &Path) {
    let path = fixture.join("observations.json");
    if !path.is_file() {
        return;
    }
    let observations: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let header = fs::read_to_string(directory.join("replay/emela_embedded.h")).unwrap();
    let mut harness = fs::read_to_string(directory.join("harness.c")).unwrap();
    let mut bindings = BTreeMap::new();
    for binding in observations["type_bindings"].as_array().unwrap() {
        let name = binding["placeholder"].as_str().unwrap();
        let function = binding["function"].as_str().unwrap();
        let prefix = binding["prefix"].as_str().unwrap();
        let declaration = header
            .lines()
            .find(|line| line.contains(&format!(" {function}(")))
            .unwrap_or_else(|| panic!("missing public declaration {function}:\n{header}"));
        bindings.insert(name.to_owned(), type_number(declaration, prefix));
    }
    if let Some(binding) = observations.get("callback_result_binding") {
        let callback = &bindings[binding["callback_placeholder"].as_str().unwrap()];
        let declaration = header
            .split(&format!("struct emela_function_type_{callback} {{"))
            .nth(1)
            .unwrap()
            .split("};")
            .next()
            .unwrap();
        let return_type = declaration.split("(*call)").nth(1).unwrap();
        bindings.insert(
            binding["placeholder"].as_str().unwrap().to_owned(),
            type_number(return_type, binding["prefix"].as_str().unwrap()),
        );
    }
    for binding in observations["release_bindings"].as_array().unwrap() {
        let ty = &bindings[binding["type_placeholder"].as_str().unwrap()];
        let declaration = header
            .lines()
            .find(|line| {
                line.starts_with("void emela_embedded_release_value_")
                    && line.contains(&format!("(emela_type_{ty} *value)"))
            })
            .unwrap_or_else(|| panic!("missing release for type {ty}:\n{header}"));
        let symbol = declaration
            .strip_prefix("void ")
            .unwrap()
            .split('(')
            .next()
            .unwrap();
        harness = harness.replace(
            &format!("{{{}}}", binding["placeholder"].as_str().unwrap()),
            symbol,
        );
    }
    for (placeholder, number) in bindings {
        harness = harness.replace(&format!("{{{placeholder}}}"), &number);
    }
    fs::write(directory.join("harness.c"), harness).unwrap();
    if observations["cancel_root"] == true {
        let path = directory.join("replay/emela_embedded.c");
        let mut source = fs::read_to_string(&path).unwrap();
        assert!(source.contains("emela_fiber_get("));
        source
            .push_str("\nvoid test_cancel(EmelaFiber h) { emela_fiber_get(h)->cancelled = 1; }\n");
        fs::write(path, source).unwrap();
    }
    if let Some(file) = observations["instrumentation_file"].as_str() {
        let path = directory.join("replay/emela_embedded.c");
        let mut source = fs::read_to_string(&path).unwrap();
        source.push_str(&fs::read_to_string(fixture.join(file)).unwrap());
        fs::write(path, source).unwrap();
    }
}

fn type_number(declaration: &str, prefix: &str) -> String {
    let number: String = declaration
        .split(prefix)
        .nth(1)
        .unwrap_or_else(|| panic!("missing {prefix} in {declaration}"))
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    assert!(!number.is_empty(), "missing type number in {declaration}");
    number
}

fn check_artifact_observations(fixture: &Path, directory: &Path, options: &serde_json::Value) {
    let path = fixture.join("observations.json");
    if !path.is_file() {
        return;
    }
    let observations: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let source = fs::read_to_string(directory.join("replay/emela_embedded.c")).unwrap();
    for (field, expected) in [("c_contains", true), ("c_not_contains", false)] {
        if let Some(values) = observations[field].as_array() {
            for value in values {
                let text = value.as_str().unwrap();
                assert_eq!(source.contains(text), expected, "{field}: {text}");
            }
        }
    }
    if let Some(count) = observations["error_show_count"].as_u64() {
        let input = emelino_compiler::BackendInput::from_json(
            &fs::read_to_string(directory.join("source/program.backend.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(input.program().error_shows.len(), count as usize);
        if let Some(limit) = observations["max_concrete_functions"].as_u64() {
            assert!(input.program().functions.len() <= limit as usize);
        }
    }
    if let Some(rejections) = observations["reject_options"].as_array() {
        for (index, rejection) in rejections.iter().enumerate() {
            let mut changed = options.clone();
            for (key, value) in rejection["override"].as_object().unwrap() {
                changed[key] = value.clone();
            }
            fs::write(
                directory.join("negative-options.json"),
                serde_json::to_vec_pretty(&changed).unwrap(),
            )
            .unwrap();
            let output_directory = format!("rejected-{index}");
            let output = cli(
                directory,
                &[
                    "build",
                    "--input",
                    "source/program.backend.json",
                    "--config",
                    "negative-options.json",
                    "--out-dir",
                    &output_directory,
                ],
            );
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains(rejection["diagnostic"].as_str().unwrap()),
                "{error}"
            );
            assert!(!error.contains("panicked"), "{error}");
            assert!(!directory.join(output_directory).exists());
        }
    }
}
