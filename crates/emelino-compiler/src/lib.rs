//! Board-independent C backend over validated, owned Flow input.
pub mod artifact;
mod backend;
mod contract;
mod input;
mod minimal;
mod semantics;
mod writer;
pub use input::BackendInput;

pub use contract::ir;
pub use contract::{
    DerivedOperation, Error, ErrorKind, ExportedFunction, ExternalAdapter, ExternalResult,
    GeneratedFile, Options, Output,
};
pub use minimal::*;

/// Compile owned Flow input without reading source files or running a frontend.
pub fn compile_program(program: ir::Program, options: &Options) -> Result<Output, Vec<Error>> {
    BackendInput::new(program)?.compile(options)
}
