//! Shared, error-tolerant source analysis for compilation and editor requests.
use emela_resolve::hir::{DefKind, Program};
use emela_resolve::{MemoryFs, ModuleId, ModuleMap, ModuleName, NoBuiltins};
use emela_syntax::ast::{Annotation, AstNode};
use la_arena::ArenaMap;
use rowan::TextRange;
use std::path::{Path, PathBuf};

/// An explicitly supplied source; names are relative module paths such as `helpers.emel`.
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

/// Embedded.* is supplied by emelino. Excluding the upstream copy makes this
/// route independent of whether a future Emela release still bundles it.
pub(crate) fn upstream_sources() -> impl Iterator<Item = (&'static str, &'static str)> {
    emela_core::core_sources()
        .iter()
        .copied()
        .filter(|(name, _)| *name != "Std.Embedded" && !name.starts_with("Std.Embedded."))
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub range: Option<TextRange>,
    pub stage: &'static str,
    pub code: Option<String>,
    pub message: String,
    /// Legacy compiler rendering, when editors use a more concise presentation.
    pub compiler_message: Option<String>,
    pub error: bool,
}

impl Diagnostic {
    fn resolved(stage: &'static str, diagnostic: &emela_resolve::Diagnostic) -> Self {
        Self {
            file: diagnostic.file.clone(),
            range: diagnostic.range,
            stage,
            code: None,
            message: diagnostic.to_string(),
            compiler_message: None,
            error: !diagnostic.kind.is_warning(),
        }
    }
}

pub struct Analysis {
    pub modules: ModuleMap,
    pub entry: ModuleId,
    pub hir: Program,
    pub types: emela_types::ProgramTypes,
    pub texts: ArenaMap<ModuleId, String>,
    pub trees: ArenaMap<ModuleId, emela_syntax::ast::SourceFile>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    /// Compilation reports the first failing stage; editors can inspect every diagnostic.
    pub fn require_valid(&self, shown_path: &Path) -> Result<(), String> {
        let Some(first) = [
            "module collection",
            "parsing",
            "source bridge",
            "import resolution",
            "name resolution",
            "type checking",
        ]
        .into_iter()
        .find_map(|stage| {
            self.diagnostics
                .iter()
                .find(|d| d.error && d.stage == stage)
        }) else {
            return Ok(());
        };
        let messages = self
            .diagnostics
            .iter()
            .filter(|d| d.error && d.stage == first.stage)
            .map(|d| {
                let file = if d.file == self.modules[self.entry].file {
                    shown_path
                } else {
                    &d.file
                };
                format!(
                    "{}:{:?}: {}",
                    file.display(),
                    d.range.unwrap_or_default(),
                    d.compiler_message.as_deref().unwrap_or(&d.message)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let producer = if first.stage == "source bridge" {
            "emelino"
        } else {
            "Emela"
        };
        Err(format!("{producer} {}:\n{messages}", first.stage))
    }
}

/// Uses the exact same resolver, shorthand completion and checker as source builds.
/// Errors in the input retain partial HIR/type tables; only structural input failures return Err.
pub fn analyze_sources(inputs: &[Source<'_>], entry_name: &str) -> Result<Analysis, String> {
    // User files never receive ModuleMap::add_core privileges. Source names are
    // explicit module paths, not a locally reimplemented project/package loader.
    let root = Path::new("<emelino-input>");
    let bundled = crate::bundled::sources();
    let mut diagnostics_out = Vec::new();
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
    diagnostics_out.extend(
        diagnostics
            .iter()
            .map(|d| Diagnostic::resolved("module collection", d)),
    );
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
    for (name, text) in upstream_sources() {
        let (id, diagnostic) = modules.add_core(
            ModuleName::parse(name),
            Path::new("<emela-core>").join(name),
        );
        if let Some(diagnostic) = diagnostic {
            diagnostics_out.push(Diagnostic::resolved("module collection", &diagnostic));
        }
        sources.insert(id, text);
    }

    let mut trees = ArenaMap::default();
    let mut imports = ArenaMap::default();

    for (id, text) in sources.iter() {
        let parsed = emela_syntax::parse(text);
        for diagnostic in parsed.diagnostics() {
            diagnostics_out.push(Diagnostic {
                file: modules[id].file.clone(),
                range: Some(diagnostic.range),
                stage: "parsing",
                code: Some(diagnostic.code.to_string()),
                message: diagnostic.message.to_string(),
                compiler_message: Some(format!("{}: {}", diagnostic.code, diagnostic.message)),
                error: true,
            });
        }
        if user_modules.contains(&id) {
            // Their semantics live in the full upstream driver, outside this
            // temporary bridge. Reject them instead of inventing local rules.
            for annotation in parsed.syntax().descendants().filter_map(Annotation::cast) {
                if let Some(name) = annotation.name()
                    && matches!(name.text(), "test" | "internal")
                {
                    diagnostics_out.push(Diagnostic {
                        file: modules[id].file.clone(),
                        range: Some(annotation.syntax().text_range()),
                        stage: "source bridge",
                        code: None,
                        message: format!("@{} requires the full upstream frontend", name.text()),
                        compiler_message: None,
                        error: true,
                    });
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
                    diagnostics_out.push(Diagnostic {
                        file: modules[id].file.clone(),
                        range: Some(annotation.syntax().text_range()),
                        stage: "source bridge",
                        code: None,
                        message:
                            "declare C exports in the emelino configuration instead of @export(c)"
                                .into(),
                        compiler_message: None,
                        error: true,
                    });
                }
            }
        }
        imports.insert(id, emela_resolve::imports(&parsed.tree()));
        trees.insert(id, parsed.tree());
    }
    let (_, diagnostics) = emela_resolve::build_import_graph(&modules, &imports);
    diagnostics_out.extend(
        diagnostics
            .iter()
            .map(|d| Diagnostic::resolved("import resolution", d)),
    );
    let (mut hir, diagnostics) = emela_resolve::resolve(&modules, &trees, &NoBuiltins);
    diagnostics_out.extend(
        diagnostics
            .iter()
            .map(|d| Diagnostic::resolved("name resolution", d)),
    );

    // These public APIs perform export placement/duplicate checks, entry
    // expansion and effect checking in the same order as Emela's driver.
    let export_diagnostics = emela_types::add_entry_exports(&mut hir, Some(entry));
    let main = hir.modules.get(entry).and_then(|module| {
        module.items.iter().copied().find(|&id| {
            let def = &hir.defs[id];
            def.name == "main" && def.kind == DefKind::Fn
        })
    });
    // Expand only the bundled peripheral shorthand. Constructors already have
    // resolved IDs; capability operations are identified by upstream inference.
    // Recheck completed calls before lowering, preserving the full signatures.
    let defaults = crate::peripheral_defaults::Defaults::new(&hir);
    defaults.complete(&mut hir, None);
    let check_options = emela_types::CheckOptions {
        main,
        tests: Vec::new(),
    };
    let mut types = emela_types::check_program_with(&hir, &check_options);
    if defaults.complete(&mut hir, Some(&types)) {
        types = emela_types::check_program_with(&hir, &check_options);
    }
    for diagnostic in types.diags.iter().chain(&export_diagnostics) {
        let (code, message) = type_message(&diagnostic.kind, &types.ty_table.cons);
        diagnostics_out.push(Diagnostic {
            file: diagnostic
                .span
                .map(|span| modules[span.module].file.clone())
                .unwrap_or_else(|| input_path.clone()),
            range: diagnostic.span.map(|span| span.range),
            stage: "type checking",
            code: code.map(str::to_owned),
            message,
            compiler_message: Some(format!("{:?}", diagnostic.kind)),
            error: !diagnostic.kind.is_warning(),
        });
    }
    let texts = sources
        .iter()
        .map(|(id, text)| (id, (*text).to_owned()))
        .collect();
    Ok(Analysis {
        modules,
        entry,
        hir,
        types,
        texts,
        trees,
        diagnostics: diagnostics_out,
    })
}

fn type_message(
    kind: &emela_types::DiagnosticKind,
    cons: &emela_types::TyCons,
) -> (Option<&'static str>, String) {
    use emela_types::DiagnosticKind as D;
    match kind {
        D::Mismatch(error) => (
            Some("E0301"),
            format!(
                "Type mismatch: expected {}, found {}",
                error.expected.display_with(cons, &[]),
                error.actual.display_with(cons, &[])
            ),
        ),
        D::WrongArgCount {
            callee,
            expected,
            found,
        } => (
            Some("E0305"),
            format!(
                "{} expects {expected} arguments, found {found}",
                callee.as_deref().unwrap_or("function")
            ),
        ),
        D::UnknownLabel { callee, label } => (
            Some("E0306"),
            format!("{callee} has no argument named `{label}`"),
        ),
        D::DuplicateArg { label } => (
            Some("E0307"),
            format!("Argument `{label}` is supplied more than once"),
        ),
        D::UnknownOperation { effect, op } => (
            Some("E0401"),
            format!("Effect {effect} has no operation `{op}`"),
        ),
        D::UnhandledEffects { effects, .. } => (
            Some("E0402"),
            format!(
                "Unhandled effects: {}",
                effects
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        D::UnhandledErrors { errors, .. } => (
            Some("E0318"),
            format!(
                "Unhandled errors: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        D::NoFieldOrFunction { ty, name } => (
            Some("E0312"),
            format!(
                "{} has no field or method `{name}`",
                ty.display_with(cons, &[])
            ),
        ),
        D::NoFields { ty, field } | D::NoField { ty, field } => (
            Some("E0312"),
            format!("{} has no field `{field}`", ty.display_with(cons, &[])),
        ),
        _ => (None, format!("{kind:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_retain_names_types_and_source_positions_but_block_lowering() {
        let source = "pub fn main() -> Int {\n  known = 42\n  unknown\n}\n";
        let result = analyze_sources(
            &[Source {
                name: "main.emel",
                text: source,
            }],
            "main.emel",
        )
        .unwrap();
        assert!(result.diagnostics.iter().any(|d| d.error));
        assert!(
            result
                .hir
                .locals
                .iter()
                .any(|(id, data)| data.name == "known" && result.types.locals.get(id).is_some())
        );
        assert_eq!(result.texts[result.entry], source);
        assert!(result.require_valid(Path::new("main.emel")).is_err());
    }

    #[test]
    fn cyclic_imports_remain_structured_diagnostics() {
        let result = analyze_sources(
            &[
                Source {
                    name: "main.emel",
                    text: "import Other\npub fn main() -> Int { 0 }",
                },
                Source {
                    name: "other.emel",
                    text: "import Main\npub fn answer() -> Int { 42 }",
                },
            ],
            "main.emel",
        )
        .unwrap();
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.stage == "import resolution" && d.error)
        );
        assert!(result.require_valid(Path::new("main.emel")).is_err());
    }
}
