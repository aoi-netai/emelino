//! Additional fixed-baseline source semantics through official Flow and saved replay.
#[path = "support/legacy.rs"]
mod legacy;
#[path = "compat/semantics/numeric_unicode.rs"]
mod numeric_unicode;
mod support;
#[path = "compat/semantics/unicode_lists.rs"]
mod unicode_lists;
