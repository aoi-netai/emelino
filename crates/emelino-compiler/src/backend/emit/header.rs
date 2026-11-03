//! Public C types, runtime interfaces, and exported function declarations.
use super::functions::signature;
use super::{HEADER_FILE, TypeLayouts};
use crate::contract::ir::{FnId, Program as Module};
use crate::{GeneratedFile, writer::Writer};
use std::collections::{HashMap, HashSet};

/// Inputs resolved by the existing module assembly pass. No input is mutated.
pub(super) struct Context<'a, 'ir> {
    pub flow: &'ir Module,
    pub layouts: &'a TypeLayouts<'ir>,
    pub emitted_functions: &'a HashSet<FnId>,

    pub public_names: &'a HashMap<FnId, String>,
    pub string_output_functions: &'a HashSet<FnId>,

    pub heap_required: bool,
    pub entry_declaration: Option<&'static str>,
}

pub(super) fn emit(context: Context<'_, '_>) -> GeneratedFile {
    let Context {
        flow,
        layouts,
        emitted_functions,

        public_names,
        string_output_functions,

        heap_required,
        entry_declaration,
    } = context;
    let mut header = Writer::new();
    header.line("#ifndef EMELA_EMBEDDED_H");
    header.line("#define EMELA_EMBEDDED_H");
    header.blank();
    header.line("#include <stdint.h>");
    header.blank();
    layouts.emit_string_declaration(&mut header);
    header.line(
        "/* Immutable bytes; length is at most INT32_MAX. owner is NULL for borrowed views. */",
    );
    header
        .line("typedef struct { const uint8_t *data; uint32_t length; void *owner; } EmelaBytes;");
    header.blank();

    if layouts.runtime.has_error_abi || layouts.runtime.any_abi {
        header.line("typedef struct EmelaError EmelaError;");
        if layouts.runtime.any_abi {
            header.line("typedef struct EmelaError EmelaAny;");
        }
        header.blank();
    }
    layouts.emit_declarations(&mut header);

    if !layouts.shapes.aggregates.is_empty() {
        header.blank();
    }

    for (id, function) in flow
        .functions
        .iter()
        .filter(|(id, f)| emitted_functions.contains(id) && f.exported)
    {
        let name = &public_names[&id];
        let takes_string_output = string_output_functions.contains(&id);
        if takes_string_output {
            header.line("/* Caller provides this output buffer; the returned EmelaString points into it. */");
        }
        header.line(&format!(
            "{};",
            signature(layouts, function, name, false, false, takes_string_output)
        ));
    }
    if let Some(declaration) = entry_declaration {
        header.line(declaration);
    }
    if heap_required {
        header.line("/* Retains a value allocated from the fixed heap. */");
        header.line("void emela_embedded_retain(void *value);");
        header.line("/* Releases a value allocated from the fixed heap. */");
        header.line("void emela_embedded_release(void *value);");
        header.line("/* Retains the backing owner of an EmelaString value. */");
        header.line("void emela_embedded_retain_string(const EmelaString *value);");
        header.line("/* Releases the backing owner of an EmelaString value. */");
        header.line("void emela_embedded_release_string(EmelaString *value);");
        header.line(
            "/* Retain/release the backing owner of a Bytes value returned by the program. */",
        );
        header.line("void emela_embedded_retain_bytes(const EmelaBytes *value);");
        header.line("void emela_embedded_release_bytes(EmelaBytes *value);");
        layouts.emit_public_release_declarations(&mut header);
        layouts.emit_public_function_ownership_declarations(&mut header);
    }

    header.blank();
    header.line("#endif");

    GeneratedFile {
        path: HEADER_FILE.into(),
        contents: header.finish(),
    }
}

use super::layouts::LayoutEmission;
