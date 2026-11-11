//! Upstream orchestration and the owned backend boundary, usable without CLI I/O.
pub mod analysis;
mod bundled;
pub mod config;
pub mod frontend;
mod peripheral_defaults;
pub mod upstream_adapter;

/// Ordinary, non-core library modules used by both the compiler and editor integration.
pub fn embedded_sources() -> &'static [(&'static str, &'static str)] {
    bundled::sources()
}

pub fn errors(errors: Vec<emelino_compiler::Error>) -> String {
    errors
        .into_iter()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn compile(source: &str, config: &config::Config) -> Result<emelino_compiler::Output, String> {
    let generated = frontend::generate(source, std::path::Path::new("main.emel"))?;
    let program = upstream_adapter::adapt(&generated.module, &config.export_pairs()?)?;
    config.validate_entry(&program)?;
    emelino_compiler::compile_program(program, &config.options()?).map_err(errors)
}
