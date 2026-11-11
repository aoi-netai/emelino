//! Source compilation over shared analysis and the pinned upstream lowering APIs.
#[cfg(test)]
pub(crate) use crate::analysis::upstream_sources;
pub use crate::analysis::{Source, analyze_sources};
use emela_core::{LowerInput, Roots};
use la_arena::ArenaMap;
use std::path::Path;

pub struct GeneratedFlow {
    pub module: emela_lir::Module,
    pub dump: String,
}

pub fn generate(source: &str, shown_path: &Path) -> Result<GeneratedFlow, String> {
    generate_sources(
        &[Source {
            name: "main.emel",
            text: source,
        }],
        "main.emel",
        shown_path,
    )
}

pub fn generate_sources(
    inputs: &[Source<'_>],
    entry_name: &str,
    shown_path: &Path,
) -> Result<GeneratedFlow, String> {
    let analysis = analyze_sources(inputs, entry_name)?;
    analysis.require_valid(shown_path)?;
    let entry = analysis.entry;
    let sources: ArenaMap<_, _> = analysis
        .texts
        .iter()
        .map(|(id, text)| (id, text.as_str()))
        .collect();
    let input = LowerInput {
        hir: &analysis.hir,
        types: &analysis.types,
        modules: &analysis.modules,
        sources,
        entry: Some(entry),
        tests: Vec::new(),
        paths: [(entry, shown_path)].into_iter().collect(),
        shown_paths: [(entry, shown_path.display().to_string())]
            .into_iter()
            .collect(),
        bundled_files: emela_core::bundled_files(),
        target: Some("c"),
    };
    // Public roots retain main and eligible public entry functions, while the
    // normal mono and reachable C external checks remain owned by upstream.
    let core = emela_core::lower(&input, Roots::Public)
        .map_err(|errors| format!("Emela Core lowering: {errors:#?}"))?
        .module;
    emela_core::verify_monomorphic(&core).map_err(|errors| {
        format!(
            "Emela Core verification: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    let flow = emela_lir::from_core(core).map_err(|errors| {
        format!(
            "Emela Flow construction: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    emela_lir::verify(&flow).map_err(|errors| {
        format!(
            "Emela Flow verification: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    Ok(GeneratedFlow {
        dump: emela_lir::dump(&flow),
        module: flow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_functions_retain_arguments_calls_and_control_flow() {
        let generated = generate("fn helper(x: Int) -> Int { x + 2 }\npub fn answer(x: Int) -> Int { if x > 0 { helper(x) } else { 42 } }", Path::new("example.emel")).unwrap();
        assert_eq!(generated.module.stage, emela_lir::Stage::Flow);
        assert!(generated.dump.contains("helper"));
        assert!(generated.dump.contains("42"));
        assert!(
            generated
                .module
                .functions
                .iter()
                .any(|(_, f)| f.name == "answer" && f.exported)
        );
    }

    #[test]
    fn diagnostics_still_belong_to_upstream() {
        for (source, expected) in [
            ("fn answer( {", "Emela parsing"),
            (
                "pub fn answer() -> Int { missing }",
                "Emela name resolution",
            ),
            (
                "pub fn answer() -> Int { \"wrong\" }",
                "Emela type checking",
            ),
        ] {
            let error = generate(source, Path::new("example.emel"))
                .err()
                .expect("invalid source accepted");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn normalized_paths_cannot_replace_user_or_extension_sources() {
        let inputs = [
            Source {
                name: "main.emel",
                text: "import Lib.Helper\npub fn answer()->Int { Helper.value() }",
            },
            Source {
                name: "lib/helper.emel",
                text: "pub fn value()->Int { 42 }",
            },
            Source {
                name: "lib//helper.emel",
                text: "pub fn value()->Int { 99 }",
            },
        ];
        let error = generate_sources(&inputs, "main.emel", Path::new("main.emel"))
            .err()
            .unwrap();
        assert!(error.contains("duplicate source name"), "{error}");
        let inputs = [Source {
            name: "embedded//gpio.emel",
            text: "pub fn fake()->Int {99}",
        }];
        let error = generate_sources(&inputs, "embedded//gpio.emel", Path::new("gpio.emel"))
            .err()
            .unwrap();
        assert!(
            error.contains("conflicts with an emelino extension"),
            "{error}"
        );
    }

    #[test]
    fn explicit_modules_use_upstream_resolution() {
        let inputs = [
            Source {
                name: "main.emel",
                text: "import Helper\npub fn answer() -> Int { Helper.value() }",
            },
            Source {
                name: "helper.emel",
                text: "pub fn value() -> Int { 42 }",
            },
        ];
        let generated = generate_sources(&inputs, "main.emel", Path::new("main.emel")).unwrap();
        assert!(generated.dump.contains("42"));
    }

    #[test]
    fn driver_attributes_and_duplicate_export_contracts_are_rejected() {
        for annotation in ["test", "internal", "export(c, \"answer\")"] {
            let source = format!("@{annotation}\npub fn answer() -> Int {{ 42 }}");
            let error = generate(&source, Path::new("example.emel"))
                .err()
                .expect("unsupported attribute accepted");
            assert!(error.contains("emelino source bridge"), "{error}");
        }
    }
}
