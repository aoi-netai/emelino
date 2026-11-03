//! C emission for the experimental, constant-returning subset of upstream Flow IR.
//!
//! This is not a complete Flow codec. A function in this profile has no parameters,
//! statements, effects, failures, suspension, const initialization, or environment.
//! The frontend must check those conditions on the actual upstream Flow module
//! before constructing this artifact. No source-language interpretation happens here.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "emelino.flow.minimal";
pub const SCHEMA_VERSION: u32 = 1;
/// The immutable commit of the Emela v0.26.1 release.
pub const UPSTREAM_REVISION: &str = "6021115103dd830e0efd9bb96a56a0ca97859329";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowArtifact {
    pub format: String,
    pub schema_version: u32,
    pub producer: Producer,
    pub stage: String,
    pub functions: Vec<FlowFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Producer {
    pub name: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowFunction {
    pub name: String,
    pub exports: Vec<String>,
    pub body: Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Terminator {
    Return { value: Constant },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Constant {
    Int { value: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CArtifacts {
    pub source: String,
    pub header: String,
}

/// Encodes only a validated artifact of the supported profile.
pub fn to_json(artifact: &FlowArtifact) -> Result<String, String> {
    validate(artifact)?;
    serde_json::to_string_pretty(artifact).map_err(|e| format!("cannot encode Flow artifact: {e}"))
}

/// Decodes and checks the complete experimental profile before C emission.
pub fn from_json(json: &str) -> Result<FlowArtifact, String> {
    let artifact =
        serde_json::from_str(json).map_err(|e| format!("invalid Flow artifact JSON: {e}"))?;
    validate(&artifact)?;
    Ok(artifact)
}

/// Emits standalone C11 functions and their header. The host owns `main`.
pub fn compile(artifact: &FlowArtifact) -> Result<CArtifacts, String> {
    validate(artifact)?;
    let mut source = String::from("#include \"program.h\"\n");
    let mut header = String::from(
        "#ifndef EMELINO_PROGRAM_H\n#define EMELINO_PROGRAM_H\n\n#include <stdint.h>\n",
    );
    for function in &artifact.functions {
        let Terminator::Return {
            value: Constant::Int { value },
        } = function.body;
        let literal = int_literal(value);
        for symbol in &function.exports {
            header.push_str(&format!("\nint32_t {symbol}(void);\n"));
            source.push_str(&format!(
                "\nint32_t {symbol}(void) {{\n    return {literal};\n}}\n"
            ));
        }
    }
    header.push_str("\n#endif\n");
    Ok(CArtifacts { source, header })
}

fn validate(artifact: &FlowArtifact) -> Result<(), String> {
    if artifact.format != FORMAT || artifact.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported Flow format/schema: {} version {} (expected {FORMAT} version {SCHEMA_VERSION})",
            artifact.format, artifact.schema_version
        ));
    }
    if artifact.producer.name != "emela" || artifact.producer.revision != UPSTREAM_REVISION {
        return Err(format!(
            "unsupported Flow producer: expected emela revision {UPSTREAM_REVISION}"
        ));
    }
    if artifact.stage != "flow" {
        return Err(format!(
            "unsupported IR stage {:?}: expected flow",
            artifact.stage
        ));
    }
    if artifact.functions.is_empty() {
        return Err("Flow artifact has no exported functions".into());
    }
    let mut functions = HashSet::new();
    let mut symbols = HashSet::new();
    for function in &artifact.functions {
        if function.name.is_empty() || !functions.insert(&function.name) {
            return Err(format!(
                "empty or duplicate Flow function name: {:?}",
                function.name
            ));
        }
        if function.exports.is_empty() {
            return Err(format!(
                "Flow function {:?} has no C exports in this profile",
                function.name
            ));
        }
        for symbol in &function.exports {
            if !valid_c_symbol(symbol) {
                return Err(format!("unsupported C export name: {symbol:?}"));
            }
            if !symbols.insert(symbol) {
                return Err(format!("duplicate C export name: {symbol:?}"));
            }
        }
    }
    Ok(())
}

fn int_literal(value: i32) -> String {
    match value {
        i32::MIN => "INT32_MIN".into(),
        n if n < 0 => format!("(-INT32_C({}))", -n),
        n => format!("INT32_C({n})"),
    }
}

fn valid_c_symbol(symbol: &str) -> bool {
    // External identifiers beginning with '_' belong to the C implementation.
    let mut bytes = symbol.bytes();
    if !bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    if matches!(
        symbol,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "main"
            | "EMELINO_PROGRAM_H"
    ) {
        return false;
    }
    // <stdint.h> owns these typedefs and macro families. Conservatively reserve
    // INT*/UINT* for macros, including implementation-provided integer widths.
    if symbol.starts_with("INT") || symbol.starts_with("UINT") {
        return false;
    }
    !matches!(
        symbol,
        "int8_t"
            | "int16_t"
            | "int32_t"
            | "int64_t"
            | "uint8_t"
            | "uint16_t"
            | "uint32_t"
            | "uint64_t"
            | "int_least8_t"
            | "int_least16_t"
            | "int_least32_t"
            | "int_least64_t"
            | "uint_least8_t"
            | "uint_least16_t"
            | "uint_least32_t"
            | "uint_least64_t"
            | "int_fast8_t"
            | "int_fast16_t"
            | "int_fast32_t"
            | "int_fast64_t"
            | "uint_fast8_t"
            | "uint_fast16_t"
            | "uint_fast32_t"
            | "uint_fast64_t"
            | "intptr_t"
            | "uintptr_t"
            | "intmax_t"
            | "uintmax_t"
            | "PTRDIFF_MIN"
            | "PTRDIFF_MAX"
            | "SIG_ATOMIC_MIN"
            | "SIG_ATOMIC_MAX"
            | "SIZE_MAX"
            | "WCHAR_MIN"
            | "WCHAR_MAX"
            | "WINT_MIN"
            | "WINT_MAX"
    )
}
