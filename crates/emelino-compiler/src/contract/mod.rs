//! Owned data contracts without a dependency on the upstream compiler.
// Shared variants are introduced before the stages that consume them.
#![allow(dead_code)]
mod derived;
mod error;
pub mod ir;
mod options;
mod output;
pub use derived::DerivedOperation;
pub use error::{Error, ErrorKind};
pub use options::{ExternalAdapter, ExternalResult, Options};
pub use output::{ExportedFunction, GeneratedFile, Output};
