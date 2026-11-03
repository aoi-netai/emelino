//! Temporary single-file bridge to the upstream compiler's public APIs.
//!
//! Parsing, resolution, typing, monomorphization and Flow construction belong to
//! Emela. This module only arranges their inputs and exports the supported Flow
//! subset. It does not parse the human-readable Flow dump.

use std::path::Path;

use emela_core::{LowerInput, Roots, TypeId};
use emela_lir::{Const, Operand, Stage, Term};
use emela_resolve::hir::DefKind;
use emela_resolve::{MemoryFs, ModuleId, ModuleMap, ModuleName, NoBuiltins};
use emela_syntax::ast::{Annotation, AstNode};
use emelino_compiler::{
    Constant, FORMAT, FlowArtifact, FlowFunction, Producer, SCHEMA_VERSION, Terminator,
    UPSTREAM_REVISION,
};
use la_arena::ArenaMap;

pub struct GeneratedFlow {
    pub artifact: FlowArtifact,
    pub dump: String,
}

pub fn generate(source: &str, shown_path: &Path) -> Result<GeneratedFlow, String> {
    // Only this source and Emela's bundled modules are inputs. User files never
    // receive the privileges granted by ModuleMap::add_core.
    let root = Path::new("<emelino-input>");
    let input_path = root.join("main.emel");
    let fs = MemoryFs::new([input_path.clone()]);
    let (mut modules, diagnostics) = emela_resolve::collect_modules(&fs, root);
    check_messages(
        "module collection",
        diagnostics.iter().map(ToString::to_string).collect(),
    )?;
    let entry = modules
        .iter()
        .find(|(_, module)| module.file == input_path)
        .map(|(id, _)| id)
        .ok_or_else(|| "Emela module collection: entry module is missing".to_owned())?;
    let mut sources = ArenaMap::default();
    sources.insert(entry, source);
    for &(name, text) in emela_core::core_sources() {
        let (id, diagnostic) = modules.add_core(
            ModuleName::parse(name),
            Path::new("<emela-core>").join(name),
        );
        if let Some(diagnostic) = diagnostic {
            return Err(format!("Emela module collection: {diagnostic}"));
        }
        sources.insert(id, text);
    }

    let mut trees = ArenaMap::default();
    let mut imports = ArenaMap::default();
    let mut parse_errors = Vec::new();
    let mut unsupported_annotations = Vec::new();
    for (id, text) in sources.iter() {
        let parsed = emela_syntax::parse(text);
        for diagnostic in parsed.diagnostics() {
            parse_errors.push(format!(
                "{}:{:?}: {}: {}",
                display_path(&modules, entry, id, shown_path),
                diagnostic.range,
                diagnostic.code,
                diagnostic.message,
            ));
        }
        if id == entry {
            // Their semantics live in the full upstream driver, outside this
            // temporary bridge. Reject them instead of inventing local rules.
            for annotation in parsed.syntax().descendants().filter_map(Annotation::cast) {
                if let Some(name) = annotation.name()
                    && matches!(name.text(), "test" | "internal")
                {
                    unsupported_annotations.push(format!(
                        "{}:{:?}: @{} requires the full upstream frontend",
                        shown_path.display(),
                        annotation.syntax().text_range(),
                        name.text(),
                    ));
                }
            }
        }
        imports.insert(id, emela_resolve::imports(&parsed.tree()));
        trees.insert(id, parsed.tree());
    }
    check_messages("parsing", parse_errors)?;
    if !unsupported_annotations.is_empty() {
        return Err(format!(
            "emelino source bridge:\n{}",
            unsupported_annotations.join("\n")
        ));
    }
    let (_, diagnostics) = emela_resolve::build_import_graph(&modules, &imports);
    check_messages(
        "import resolution",
        diagnostics.iter().map(ToString::to_string).collect(),
    )?;
    let (mut hir, diagnostics) = emela_resolve::resolve(&modules, &trees, &NoBuiltins);
    check_messages(
        "name resolution",
        diagnostics
            .iter()
            .filter(|diagnostic| !diagnostic.kind.is_warning())
            .map(|diagnostic| {
                let path = if diagnostic.file == input_path {
                    shown_path
                } else {
                    &diagnostic.file
                };
                format!("{}:{:?}: {diagnostic}", path.display(), diagnostic.range)
            })
            .collect(),
    )?;

    // These public APIs perform export placement/duplicate checks, entry
    // expansion and effect checking in the same order as Emela's driver.
    let export_diagnostics = emela_types::add_entry_exports(&mut hir, Some(entry));
    let main = hir.modules.get(entry).and_then(|module| {
        module.items.iter().copied().find(|&id| {
            let def = &hir.defs[id];
            def.name == "main" && def.kind == DefKind::Fn
        })
    });
    let types = emela_types::check_program_with(
        &hir,
        &emela_types::CheckOptions {
            main,
            tests: Vec::new(),
        },
    );
    check_messages(
        "type checking",
        types
            .diags
            .iter()
            .chain(&export_diagnostics)
            .filter(|diagnostic| !diagnostic.kind.is_warning())
            .map(|diagnostic| {
                let location = diagnostic.span.map_or_else(
                    || "<no source>".to_owned(),
                    |span| {
                        format!(
                            "{}:{:?}",
                            display_path(&modules, entry, span.module, shown_path),
                            span.range,
                        )
                    },
                );
                format!("{location}: {:?}", diagnostic.kind)
            })
            .collect(),
    )?;

    let input = LowerInput {
        hir: &hir,
        types: &types,
        modules: &modules,
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
    // A target and Exported roots make upstream lower run its standard
    // monomorphization and target checks. Do not run a second mono pass here.
    let core = emela_core::lower(&input, Roots::Exported)
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
        artifact: export_minimal(&flow)?,
        dump: emela_lir::dump(&flow),
    })
}

fn export_minimal(flow: &emela_lir::Module) -> Result<FlowArtifact, String> {
    if flow.stage != Stage::Flow || !flow.error_shows.is_empty() {
        return Err(
            "emelino Flow export: only Flow without error display functions is supported"
                .to_owned(),
        );
    }
    // from_core publicly guarantees that Core functions occupy the initial
    // Flow slots in the same order and with the same names. Only declaration
    // metadata is read here; Core function bodies are never examined.
    let declarations: Vec<_> = flow.core.functions.iter().map(|(_, f)| f).collect();
    let mut functions = Vec::new();
    for (index, (_, function)) in flow.functions.iter().enumerate() {
        let unsupported = || {
            format!(
                "emelino Flow export: function `{}` is outside the minimal profile (pure, non-const, no arguments, Int literal return)",
                function.name,
            )
        };
        if !function.params.is_empty()
            || function.ret != TypeId::INT
            || function.fails
            || function.suspends
            || function.constant
            || function.external.is_some()
            || function.env.is_some()
            || function.defects
            || !function.vars.is_empty()
            || !function.stmts.is_empty()
            || !function.labels.is_empty()
            || function.seqs.len() != 1
        {
            return Err(unsupported());
        }
        let sequence = function
            .body
            .map(|body| &function.seqs[body])
            .ok_or_else(unsupported)?;
        let Term::Return(Some(Operand::Const(Const::Int(value)))) = sequence.term else {
            return Err(unsupported());
        };
        if !sequence.stmts.is_empty() {
            return Err(unsupported());
        }
        let declaration = declarations.get(index).ok_or_else(unsupported)?;
        if declaration.name != function.name || declaration.exported != function.exported {
            return Err(
                "emelino Flow export: upstream function metadata does not match its Flow slot"
                    .to_owned(),
            );
        }
        let mut exports = Vec::new();
        for export in &declaration.host_exports {
            if export.target != "c" || export.interface.is_some() {
                return Err(unsupported());
            }
            exports.push(export.name.clone());
        }
        // An implicit main requires launcher semantics, which this experiment
        // does not implement. Every exposed root must have explicit C names.
        if function.exported && exports.is_empty() {
            return Err(format!(
                "emelino Flow export: root `{}` needs an explicit @export(c, \"name\")",
                function.name,
            ));
        }
        functions.push(FlowFunction {
            name: function.name.clone(),
            exports,
            body: Terminator::Return {
                value: Constant::Int { value },
            },
        });
    }
    Ok(FlowArtifact {
        format: FORMAT.to_owned(),
        schema_version: SCHEMA_VERSION,
        producer: Producer {
            name: "emela".to_owned(),
            revision: UPSTREAM_REVISION.to_owned(),
        },
        stage: "flow".to_owned(),
        functions,
    })
}

fn display_path(modules: &ModuleMap, entry: ModuleId, id: ModuleId, shown: &Path) -> String {
    if id == entry {
        shown.display().to_string()
    } else {
        modules[id].file.display().to_string()
    }
}

fn check_messages(stage: &str, messages: Vec<String>) -> Result<(), String> {
    if messages.is_empty() {
        Ok(())
    } else {
        Err(format!("Emela {stage}:\n{}", messages.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test(source: &str) -> Result<GeneratedFlow, String> {
        generate(source, Path::new("example.emel"))
    }

    #[test]
    fn upstream_flow_preserves_literal_and_host_export_name() {
        let generated =
            generate_test("@export(c, \"board_answer\")\npub fn answer() -> Int { 42 }\n").unwrap();
        assert_eq!(generated.artifact.functions.len(), 1);
        let function = &generated.artifact.functions[0];
        assert_eq!(function.exports, ["board_answer"]);
        assert!(matches!(
            function.body,
            Terminator::Return {
                value: Constant::Int { value: 42 }
            }
        ));
        assert!(generated.dump.contains("42"));
        let changed =
            generate_test("@export(c, \"answer\")\npub fn answer() -> Int { 7 }\n").unwrap();
        assert!(matches!(
            changed.artifact.functions[0].body,
            Terminator::Return {
                value: Constant::Int { value: 7 }
            }
        ));
    }

    #[test]
    fn upstream_errors_and_unsupported_flow_are_distinct() {
        let cases = [
            ("fn answer( {", "Emela parsing"),
            (
                "@export(c, \"answer\")\npub fn answer() -> Int { missing }\n",
                "Emela name resolution",
            ),
            (
                "@export(c, \"answer\")\npub fn answer() -> Int { \"wrong\" }\n",
                "Emela type checking",
            ),
            (
                "@export(c, \"answer\")\npub fn answer() -> Int { 40 + 2 }\n",
                "emelino Flow export",
            ),
            (
                "@export(c, \"answer\")\npub fn answer(value: Int) -> Int { value }\n",
                "emelino Flow export",
            ),
        ];
        for (source, expected) in cases {
            let error = match generate_test(source) {
                Ok(_) => panic!("unexpectedly accepted: {source}"),
                Err(error) => error,
            };
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }

    #[test]
    fn duplicate_exports_are_checked_by_upstream() {
        let source = "@export(c, \"answer\")\nfn first() -> Int { 42 }\n\n@export(c, \"answer\")\nfn second() -> Int { 7 }\n";
        let error = match generate_test(source) {
            Ok(_) => panic!("duplicate export accepted"),
            Err(error) => error,
        };
        assert!(error.contains("DuplicateExport"), "{error}");
    }

    #[test]
    fn driver_only_annotations_are_not_silently_ignored() {
        for annotation in ["test", "internal"] {
            let source =
                format!("@{annotation}\n@export(c, \"answer\")\nfn answer() -> Int {{ 42 }}\n");
            let error = match generate_test(&source) {
                Ok(_) => panic!("@{annotation} accepted without the upstream driver"),
                Err(error) => error,
            };
            assert!(error.contains("emelino source bridge"), "{error}");
        }
    }
}
