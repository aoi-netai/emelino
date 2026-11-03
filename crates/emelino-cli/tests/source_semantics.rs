//! Additional fixed-baseline source semantics through official Flow and saved replay.
#[path = "compat/semantics/evaluation.rs"]
mod evaluation;
#[path = "support/legacy.rs"]
mod legacy;
#[path = "compat/semantics/numeric_unicode.rs"]
mod numeric_unicode;
#[path = "compat/semantics/parse.rs"]
mod parse;
#[path = "compat/semantics/standard_api.rs"]
mod standard_api;
mod support;
#[path = "compat/semantics/unicode_lists.rs"]
mod unicode_lists;
