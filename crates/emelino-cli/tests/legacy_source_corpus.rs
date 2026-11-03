//! All fixed legacy source programs reach C generation through official Flow.
//! Runtime observations are tracked separately in reused_* and legacy_runtime.
use emelino_cli::{frontend, upstream_adapter};
use emelino_compiler::{ExternalAdapter, ExternalResult, Options, ir};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn compile_case(program: &Value, directory: &Path, concrete_entry: bool) -> Result<usize, String> {
    let original_entry = Path::new(program["entry"].as_str().unwrap());
    let source_root = original_entry.parent().unwrap_or(Path::new(""));
    let entry = original_entry
        .strip_prefix(source_root)
        .unwrap()
        .to_str()
        .unwrap();
    let owned: Vec<_> = program["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            let name = Path::new(file["name"].as_str().unwrap())
                .strip_prefix(source_root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let text =
                std::fs::read_to_string(directory.join(file["fixture"].as_str().unwrap())).unwrap();
            // Bytes was a fixed fork extension. Its pure Emela implementation now
            // belongs to Emelino.Bytes, supplied as an ordinary external module.
            let text: String = text
                .split_inclusive('\n')
                .map(|line| {
                    if line.trim_start().starts_with("import Std.Bytes") {
                        line.replacen("Std.Bytes", "Emelino.Bytes", 1)
                    } else {
                        line.to_owned()
                    }
                })
                .collect();
            let text = if concrete_entry {
                assert_eq!(program["id"], "program:45d10a963a24d47032c7");
                let original = "  reader = escaped(n)\n  reader() + reader()";
                assert_eq!(text.matches(original).count(), 1);
                text.replace(original, "  (result, _) = State.run(0, fn() {\n    reader = escaped(n)\n    reader() + reader()\n  })\n  result")
            } else {
                text
            };
            (name, text)
        })
        .collect();
    let sources: Vec<_> = owned
        .iter()
        .map(|(name, text)| frontend::Source { name, text })
        .collect();
    let flow = frontend::generate_sources(&sources, entry, Path::new(entry))?;
    let exports: Vec<_> = flow
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.exported)
        .enumerate()
        .map(|(index, (_, function))| (function.name.clone(), format!("corpus_export_{index}")))
        .collect();
    if exports.is_empty() {
        return Err("fixed source program has no public Flow roots".into());
    }
    let program = upstream_adapter::adapt(&flow.module, &exports)?;
    // This checks language connection, not the old tests' resource bounds. The
    // runtime harnesses separately retain each original capacity and assertion.
    let mut options = Options::default()
        .with_entry("")
        .with_heap_bytes(1_048_576)
        .with_frame_bytes(8192)
        .with_frame_capacity(64)
        .with_fiber_capacity(32)
        .with_deferred_capacity(32)
        .with_string_capacity(512);
    let mut adapters = BTreeSet::new();
    for (_, function) in program.functions.iter() {
        let Some(binding) = function
            .external
            .as_ref()
            .and_then(|external| external.binding("c"))
        else {
            continue;
        };
        let ir::HostRef::Global(symbol) = &binding.host else {
            continue;
        };
        if symbol.starts_with("__emela_") || !adapters.insert(symbol.clone()) {
            continue;
        }
        if function.suspends {
            options = options.with_task_adapter(symbol, format!("{symbol}_adapter"));
        } else {
            options = options.with_external_adapter(
                symbol,
                ExternalAdapter::new(symbol, ExternalResult::Borrowed),
            );
        }
    }
    let output =
        emelino_compiler::compile_program(program, &options).map_err(emelino_cli::errors)?;
    if output.c_source.is_empty() || output.exports.len() != exports.len() {
        return Err("generated artifact omitted configured public roots".into());
    }
    Ok(exports.len())
}

#[test]
fn every_fixed_legacy_source_program_reaches_c_generation() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/compat/legacy-corpus");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["baseline_revision"],
        "c34c0aeefdd966d854371b3110db00a47798751d"
    );
    let programs = manifest["programs"].as_array().unwrap();
    assert_eq!(programs.len(), 169);
    let mut files = BTreeSet::new();
    let mut legacy_tests = BTreeSet::new();
    let mut failures = Vec::new();
    let mut public_functions = 0;
    let mut concrete_wrappers = 0;
    for program in programs {
        for file in program["files"].as_array().unwrap() {
            files.insert(file["fixture"].as_str().unwrap());
        }
        for test in program["legacy_tests"].as_array().unwrap() {
            legacy_tests.insert(test.as_str().unwrap());
        }
        if program["id"] == "program:45d10a963a24d47032c7" {
            // The original inferred State[Int] effect is not a concrete C entry.
            // Check the diagnostic first, then supply exactly the handler wrapper
            // exercised by legacy_runtime. Do not count the original as accepted.
            let error = compile_case(program, &directory, false).unwrap_err();
            assert!(
                error.contains("`escaped_result` has unresolved effect evidence"),
                "{error}"
            );
            match compile_case(program, &directory, true) {
                Ok(count) => {
                    public_functions += count;
                    concrete_wrappers += 1;
                }
                Err(error) => failures.push(format!(
                    "{} wrapper: {error}",
                    program["id"].as_str().unwrap()
                )),
            }
            continue;
        }
        match compile_case(program, &directory, false) {
            Ok(count) => public_functions += count,
            Err(error) => failures.push(format!("{}: {error}", program["id"].as_str().unwrap())),
        }
    }
    assert_eq!(files.len(), 170);
    assert_eq!(legacy_tests.len(), 174);
    assert!(
        failures.is_empty(),
        "{} of 169 source programs failed:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(public_functions >= 169);
    assert_eq!(concrete_wrappers, 1);
}
