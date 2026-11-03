//! Reject features whose implementation has not been introduced yet.
use crate::contract::ir::Program;
use crate::{Error, ErrorKind, Options};
pub(super) fn validate(program: &Program, options: &Options) -> Result<(), Vec<Error>> {
    if !options.external_adapters().is_empty()
        || !options.isr_entries().is_empty()
        || !options.isr_safe_externals().is_empty()
    {
        return Err(vec![Error {
            kind: ErrorKind::Unsupported,
            message: "external adapters and ISR entries are not supported by this backend".into(),
        }]);
    }
    for (_, function) in program.functions.iter() {
        if function.canonical_const.is_some() {
            return Err(vec![Error {
                kind: ErrorKind::Unsupported,
                message: format!(
                    "function `{}`: shared const families are not supported by this backend",
                    function.name
                ),
            }]);
        }
    }
    Ok(())
}
