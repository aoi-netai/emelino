//! Board-independent C backend over validated, owned Flow input.
pub mod artifact;
mod backend;
mod contract;
mod semantics;
mod writer;
mod minimal;

pub use minimal::*;
pub use contract::ir;
pub use contract::{DerivedOperation, Error, ErrorKind, ExportedFunction, ExternalAdapter, ExternalResult, GeneratedFile, Options, Output};

/// Compile owned Flow input without reading source files or running a frontend.
pub fn compile_program(program: ir::Program, options: &Options) -> Result<Output, Vec<Error>> {
    backend::compile_program(program, options)
}
