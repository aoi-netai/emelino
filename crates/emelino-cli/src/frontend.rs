//! Source collection and orchestration over the pinned upstream public APIs.
//!
//! Parsing, resolution, typing, monomorphization and Flow construction belong to
//! Emela. This module only arranges explicitly supplied inputs. It does not
//! inspect Core function bodies or parse the human-readable Flow dump.

use std::path::Path;

use emela_core::{LowerInput, Roots};
use emela_resolve::hir::DefKind;
use emela_resolve::{MemoryFs, ModuleId, ModuleMap, ModuleName, NoBuiltins};
use emela_syntax::ast::{Annotation, AstNode};
use la_arena::ArenaMap;

/// An explicitly supplied source; names are relative module paths such as `helpers.emel`.
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

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
    // User files never receive ModuleMap::add_core privileges. Source names are
    // explicit module paths, not a locally reimplemented project/package loader.
    let root = Path::new("<emelino-input>");
    let bundled = crate::bundled::sources()?;
    if inputs.iter().any(|input| {
        bundled
            .iter()
            .any(|(name, _)| Path::new(name) == Path::new(input.name))
    }) {
        return Err("source name conflicts with an emelino extension module".into());
    }
    let inputs: Vec<_> = inputs
        .iter()
        .map(|source| Source {
            name: source.name,
            text: source.text,
        })
        .chain(bundled.iter().map(|(name, text)| Source { name, text }))
        .collect();
    let mut names = std::collections::HashSet::new();
    for input in &inputs {
        let path = Path::new(input.name);
        if path.extension().is_none_or(|ext| ext != "emel")
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || !names.insert(path.to_path_buf())
        {
            return Err(format!("invalid or duplicate source name: {}", input.name));
        }
    }
    if !names.contains(Path::new(entry_name)) {
        return Err(format!("entry source is not supplied: {entry_name}"));
    }
    let input_path = root.join(entry_name);
    let fs = MemoryFs::new(inputs.iter().map(|source| root.join(source.name)));
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
    let mut user_modules = std::collections::HashSet::new();
    for input in &inputs {
        let file = root.join(input.name);
        let id = modules
            .iter()
            .find(|(_, module)| module.file == file)
            .map(|(id, _)| id)
            .ok_or_else(|| format!("Emela module collection: source is missing: {}", input.name))?;
        sources.insert(id, input.text);
        user_modules.insert(id);
    }
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
        if user_modules.contains(&id) {
            // Their semantics live in the full upstream driver, outside this
            // temporary bridge. Reject them instead of inventing local rules.
            for annotation in parsed.syntax().descendants().filter_map(Annotation::cast) {
                if let Some(name) = annotation.name()
                    && matches!(name.text(), "test" | "internal")
                {
                    unsupported_annotations.push(format!(
                        "{}:{:?}: @{} requires the full upstream frontend",
                        display_path(&modules, entry, id, shown_path),
                        annotation.syntax().text_range(),
                        name.text(),
                    ));
                }
                if annotation
                    .name()
                    .is_some_and(|name| name.text() == "export")
                    && annotation.args().next().is_some_and(|arg| {
                        arg.descendants_with_tokens()
                            .filter_map(|node| node.into_token())
                            .any(|token| {
                                token.kind() == emela_syntax::SyntaxKind::LOWER_NAME
                                    && token.text() == "c"
                            })
                    })
                {
                    unsupported_annotations.push(format!("{}:{:?}: declare C exports in the emelino configuration instead of @export(c)",
                        display_path(&modules, entry, id, shown_path), annotation.syntax().text_range()));
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
