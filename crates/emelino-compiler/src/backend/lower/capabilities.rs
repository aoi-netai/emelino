//! Reject features whose implementation has not been introduced yet.
use crate::contract::ir::Program;
use crate::{Error, ErrorKind, Options};
pub(super) fn validate(_program: &Program, options: &Options) -> Result<(), Vec<Error>> {
    if !options.external_adapters().is_empty()
        || !options.isr_entries().is_empty()
        || !options.isr_safe_externals().is_empty()
    {
        return Err(vec![Error {
            kind: ErrorKind::Unsupported,
            message: "external adapters and ISR entries are not supported by this backend".into(),
        }]);
    }

    Ok(())
}
