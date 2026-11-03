//! Board-independent C generation over validated, owned Flow input.
pub mod artifact;
mod contract;
mod input;
mod minimal;
mod semantics;
pub use contract::ir;
pub use contract::{
    DerivedOperation, Error, ErrorKind, ExportedFunction, ExternalAdapter, ExternalResult,
    GeneratedFile, Options, Output,
};
pub use minimal::*;

pub use input::BackendInput;
