//! C formatting of a complete, immutable backend plan.
use crate::contract::abi::{AggregateKind, CLayoutPlan as TypeLayouts};
use crate::contract::ir::*;
use crate::contract::strings::StringBuffers as StringBufferPlan;
use crate::semantics::abi::{LayoutRead, option_payload_type};
use crate::semantics::c::variable_name;
use crate::semantics::diagnostics::{
    self, invalid_flow_ir, statement_name, term_name, unsupported, value_name,
};
use crate::{Error, ErrorKind, Options};
use std::collections::{HashMap, HashSet};

use crate::contract::planned::PlannedModule;

pub(super) fn render(plan: &PlannedModule<'_>) -> Result<Output, Vec<Error>> {
    let options = &plan.options;
    let compiled = plan.input;
    let flow = &compiled.lowered;
    let closures = &compiled.closures;
    let runtime_consts = &compiled.runtime_consts;
    let layouts = &plan.layouts;
    let external_bindings = &plan.external_bindings;
    let _const_bridges = &plan.const_bridges;
    let _derived_plan = &plan.derived_plan;
    let emitted_functions = &plan.emitted_functions;
    let heap_functions = &plan.heap_functions;
    let string_output_functions = &plan.string_output_functions;
    let _const_outcomes = &plan.const_outcomes;
    let names = &plan.names;
    let public_names = &plan.public_names;
    let runtime = &plan.runtime;
    let _has_scheduler = plan.has_scheduler;
    let has_tasks = plan.has_tasks;
    let _task_value_types = &plan.task_value_types;
    let heap_required = !heap_functions.is_empty();
    let main_entry = plan.main_entry.map(|id| (id, &flow.functions[id]));
    let target_headers: Vec<GeneratedFile> = Vec::new();
    let _function_ref_adapters = &plan.function_ref_adapters;
    let header = header::emit(header::Context {
        flow,
        layouts,
        emitted_functions,

        public_names,
        string_output_functions,

        heap_required,
        entry_declaration: main_entry.map(|_| "void emela_embedded_start(void);"),
    });

    let platform_header = platform::header(platform::Requirements {
        defects: runtime.defects,
        const_defects: !runtime_consts.is_empty(),
        lifecycle: main_entry.is_some(),
        debug: runtime.debug,
        event_lock: has_tasks,
    });

    let mut source = Writer::new();
    source.line(&format!("#include \"{HEADER_FILE}\""));
    source.line(&format!("#include \"{}\"", platform_header.path));
    source.line("#include <math.h>");
    source.line("#include <stddef.h>");
    for file in &target_headers {
        source.line(&format!("#include \"{}\"", file.path));
    }
    source.blank();
    c::emit_unused_attribute(&mut source);
    source.blank();

    layouts.emit_runtime_value_declarations(&mut source);
    if heap_required {
        layouts.emit_list_node_declarations(&mut source);
        crate::backend::emit::heap::emit_runtime(
            &mut source,
            options.heap_bytes(),
            layouts.has_cell_cycles(),
        );

        emit_closure_string_copy_helper(layouts, &mut source);
        bytes_runtime::emit(layouts, &mut source);

        emit_any_ownership_declarations(layouts, &mut source);
        layouts.emit_heap_ownership_helpers(&mut source);
        owned::emit_helpers(layouts, &mut source);

        emit_any_ownership(layouts, true, &mut source);

        layouts.emit_heap_destroy_switch(&closures.environments, &mut source);
        layouts.emit_public_release_helpers(&mut source);
        layouts.emit_public_function_ownership_helpers(&mut source);
        crate::backend::emit::heap::emit_public_retain(&mut source);
        crate::backend::emit::heap::emit_public_release(&mut source);
        emit_public_string_ownership(&mut source);
    } else {
        source.line(
            "static inline EMELA_UNUSED void emela_heap_retain(void *value) { (void)value; }",
        );
        source.line(
            "static inline EMELA_UNUSED void emela_heap_release(void *value) { (void)value; }",
        );

        emit_any_ownership(layouts, false, &mut source);
        layouts.emit_noheap_ownership_stubs(&mut source);
        source.blank();
    }

    runtime.emit_helpers(&mut source)?;

    for (id, function) in flow
        .functions
        .iter()
        .filter(|(id, _)| emitted_functions.contains(id))
    {
        let name = &names[&id];
        let takes_string_output = string_output_functions.contains(&id);
        if function.external.is_some() {
            let prefix = if external_bindings[&id].adapted {
                ""
            } else {
                "extern "
            };
            source.line(&format!(
                "{prefix}{};",
                signature(
                    layouts,
                    function,
                    name,
                    external_bindings[&id].adapted,
                    function.fails && external_bindings[&id].adapted,
                    string_output_functions.contains(&id)
                )
            ));
        } else if !function.exported || function.fails {
            source.line(&format!(
                "{};",
                signature(
                    layouts,
                    function,
                    name,
                    true,
                    function.fails,
                    takes_string_output,
                )
            ));
        }
    }

    source.blank();

    for (id, function) in flow
        .functions
        .iter()
        .filter(|(id, _)| emitted_functions.contains(id))
    {
        if function.external.is_some() {
            if function.body.is_some() {
                return Err(vec![Error {
                    kind: ErrorKind::InvalidFlowIr,
                    message: format!(
                        "invalid Flow IR in function `{}`: external function has a body",
                        function.name
                    ),
                }]);
            }

            continue;
        }
        let Some(body) = function.body else {
            return Err(unsupported(&function.name, "function without a body"));
        };
        let sequence = &function.seqs[body];

        let _initializer_name = format!("{}_const_init", names[&id]);
        let is_runtime_const = runtime_consts.contains(&id);
        let name = &names[&id];
        let is_static = is_runtime_const || !function.exported || function.fails;
        let takes_string_output = string_output_functions.contains(&id);
        let function_signature = signature(
            layouts,
            function,
            name,
            is_static,
            function.fails,
            takes_string_output,
        );
        source.begin_block(&function_signature);

        if layouts.runtime.has_cause_raise && layouts.runtime.has_unwind {
            source.line("unsigned emela_entry_holds = emela_current_fiber == NULL ? 0 : emela_current_fiber->holds;");
            source.line("(void)emela_entry_holds;");
        }
        if function.fails {
            source.line("*emela_error_out = (EmelaError){0};");
        }
        if takes_string_output {
            source.decl(
                "uint16_t",
                "emela_emb_string_output_capacity",
                Some(&format!(
                    "emela_string_capacity < UINT16_C({}) ? emela_string_capacity : UINT16_C({})",
                    options.string_capacity(),
                    options.string_capacity()
                )),
            );
            // A String result can still have no returning path (for example a
            // function whose body always fails). Keep its output ABI uniform.
            source.line("(void)emela_emb_string_output_capacity;");
        }
        let string_buffers = &plan.strings[&id];
        emit_local_declarations(layouts, function, string_buffers, &mut source);
        if let Some(&_environment) = layouts.functions.closure_functions.get(&id) {}
        let mut loops = Vec::new();
        let mut blocks = Vec::new();
        let mut protects = Vec::new();
        let mut next_temp = 0;
        let mut next_label = 0;
        for &statement in &sequence.stmts {
            emit_statement(
                layouts,
                function,
                &function.stmts[statement],
                &mut source,
                names,
                string_buffers,
                &mut loops,
                &mut blocks,
                &mut protects,
                &mut next_temp,
                &mut next_label,
            )?;
        }
        emit_return(
            layouts,
            function,
            &sequence.term,
            string_buffers,
            &mut source,
        )?;
        source.end_block();
        source.blank();
    }

    if let Some((id, _)) = main_entry {
        entry::emit(plan, id, &mut source);
    }
    let mut output = Output {
        c_source: String::new(),
        header: String::new(),
        exports: plan.metadata.exports.clone(),
        files: vec![
            header,
            platform_header,
            GeneratedFile {
                path: SOURCE_FILE.to_owned(),
                contents: source.finish(),
            },
        ]
        .into_iter()
        .chain(target_headers)
        .collect(),
    };
    let mut alias_header = String::from(
        "#ifndef EMELA_EMBEDDED_EXPORTS_H\n#define EMELA_EMBEDDED_EXPORTS_H\n\n#include \"emela_embedded.h\"\n",
    );
    for (alias, symbol) in &plan.aliases {
        alias_header.push_str(&format!("\n#ifdef {alias}\n#error \"embedded export alias {alias} conflicts with an existing macro\"\n#endif\n#define {alias} {symbol}\n"));
    }
    alias_header.push_str("\n#endif\n");
    output.files.push(GeneratedFile {
        path: crate::artifact::ALIASES_HEADER.into(),
        contents: alias_header,
    });
    let mut manifest = crate::artifact::Manifest::from_files(&output.files)
        .map_err(|error| vec![Error::from(error)])?;
    manifest.metadata = plan.metadata.clone();
    output.files.push(GeneratedFile {
        path: crate::artifact::MANIFEST_FILE.into(),
        contents: manifest
            .to_json()
            .map_err(|error| vec![Error::from(error)])?,
    });
    output
        .refresh_manifest()
        .map_err(|error| vec![Error::from(error)])?;
    output.c_source = output
        .files
        .iter()
        .find(|f| f.path == SOURCE_FILE)
        .expect("primary source is emitted")
        .contents
        .clone();
    output.header = output
        .files
        .iter()
        .find(|f| f.path == HEADER_FILE)
        .expect("primary header is emitted")
        .contents
        .clone();
    Ok(output)
}

mod control;
mod functions;
mod header;
mod values;

use control::{
    BlockTarget, ProtectTarget, emit_call_failure_edge, emit_compound_sequence,
    emit_error_assignment, emit_failure_return, emit_pending_cleanups, emit_return, emit_statement,
};
use functions::signature;
use functions::{
    emit_any_ownership, emit_any_ownership_declarations, emit_closure_string_copy_helper,
    emit_local_declarations, emit_owned_cleanup, emit_public_string_ownership,
    emit_release_if_owned, emit_retain_operand,
};
use values::{
    constant_expression, emit_aggregate_field_store, emit_builtin, emit_runtime,
    operand_expression, operand_is_float, operand_is_int, operand_is_int64, operand_is_string,
    operand_is_string_list, option_variant_expression, primitive_expression,
};

pub(super) mod layouts;

pub(super) mod string_buffers;

pub(super) mod runtime_helpers;
use runtime_helpers::RuntimeEmission;

mod features;
use crate::writer::Writer;
use crate::{GeneratedFile, Output};
mod c;
mod numeric_runtime;
mod owned;
mod platform;
mod string_runtime;
use crate::contract::operations as analysis;
use c::*;
use layouts::LayoutEmission;
use string_buffers::StringBufferEmission;
const HEADER_FILE: &str = "emela_embedded.h";
const SOURCE_FILE: &str = "emela_embedded.c";

mod entry;

mod bytes_runtime;
mod heap;

mod float_parse;

mod float_to_string;

use crate::semantics::operands::operand_type;

#[cfg(test)]
mod tests {
    #[test]
    fn renders_a_contract_fixture_without_invoking_plan_or_lower() {
        let input = crate::contract::fixtures::lowered();
        let plan = crate::contract::fixtures::planned(&input);
        let first = super::render(&plan).unwrap();
        let second = super::render(&plan).unwrap();
        assert_eq!(first.files, second.files);
        assert!(
            first
                .files
                .iter()
                .any(|file| file.path == crate::artifact::MANIFEST_FILE)
        );
        assert!(first.exports.is_empty());
    }
}
