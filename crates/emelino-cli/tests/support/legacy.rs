//! Test-only migration of explicit old fixture exports into the new configuration.
//! Parsing uses upstream AST; source bodies are not lowered by a local frontend.
#![allow(dead_code)]
use emela_syntax::ast::{AstNode, FnDecl, HasAttrs};
use emelino_compiler::{Error, ErrorKind};
#[allow(unused_imports)]
pub use emelino_compiler::{ExternalAdapter, ExternalResult, Options, Output};
use std::path::Path;

pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}
impl<'a> Source<'a> {
    pub fn new(name: &'a str, text: &'a str) -> Self {
        Self { name, text }
    }
}
fn error(message: impl Into<String>) -> Vec<Error> {
    vec![Error {
        kind: ErrorKind::InvalidFlowIr,
        message: message.into(),
    }]
}

pub fn compile_sources(
    sources: &[Source<'_>],
    entry: &str,
    options: &Options,
) -> Result<Output, Vec<Error>> {
    let mut exports = Vec::new();
    let mut adapted = Vec::new();
    let base = Path::new(entry).parent().unwrap_or(Path::new(""));
    for input in sources {
        // Standard-library imports pass unchanged to the pinned upstream.
        let source = input.text.to_owned();
        let parsed = emela_syntax::parse(&source);
        if !parsed.diagnostics().is_empty() {
            return Err(error(format!(
                "legacy fixture parsing: {:?}",
                parsed.diagnostics()
            )));
        }
        let mut ranges = Vec::new();
        for function in parsed.syntax().descendants().filter_map(FnDecl::cast) {
            for annotation in function.annotations() {
                if !annotation
                    .name()
                    .is_some_and(|name| name.text() == "export")
                {
                    continue;
                }
                let args: Vec<_> = annotation
                    .args()
                    .map(|arg| arg.text().to_string())
                    .collect();
                if args.first().is_some_and(|target| target.trim() == "c") {
                    if input.name != entry || args.len() != 2 {
                        return Err(error("unexpected legacy fixture export shape"));
                    }
                    let symbol: String =
                        serde_json::from_str(args[1].trim()).map_err(|e| error(e.to_string()))?;
                    exports.push((
                        function
                            .name()
                            .ok_or_else(|| error("missing fixture function name"))?
                            .text()
                            .to_owned(),
                        symbol,
                    ));
                    let range = annotation.syntax().text_range();
                    ranges.push(usize::from(range.start())..usize::from(range.end()));
                }
            }
        }
        let mut source = source;
        ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
        for range in ranges {
            source.replace_range(range, "");
        }
        let name = Path::new(input.name)
            .strip_prefix(base)
            .map_err(|e| error(e.to_string()))?
            .to_str()
            .ok_or_else(|| error("invalid fixture path"))?
            .to_owned();
        adapted.push((name, source));
    }
    let inputs: Vec<_> = adapted
        .iter()
        .map(|(name, text)| emelino_cli::frontend::Source { name, text })
        .collect();
    let entry_name = Path::new(entry)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| error("invalid fixture entry"))?;
    let generated = emelino_cli::frontend::generate_sources(&inputs, entry_name, Path::new(entry))
        .map_err(error)?;
    let program =
        emelino_cli::upstream_adapter::adapt(&generated.module, &exports).map_err(error)?;
    let input = emelino_compiler::BackendInput::new(program)?;
    let output = input.compile(options)?;
    let replay = emelino_compiler::BackendInput::from_json(&input.to_json()?)?.compile(options)?;
    assert_eq!(
        output, replay,
        "saved legacy fixture changed generated artifacts"
    );
    Ok(output)
}
