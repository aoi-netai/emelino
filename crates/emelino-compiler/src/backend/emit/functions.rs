//! Function signatures, ownership helpers, closures, and public wrappers.
use super::{
    StringBufferPlan, TypeLayouts,
    c::{owned_variable_name, variable_name},
};
use crate::contract::ir::{Function, Operand, Repr, Stmt, VarId};
use crate::semantics::abi::LayoutRead;
use crate::writer::Writer;

pub(in crate::backend::emit) fn emit_local_declarations(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    string_buffers: &StringBufferPlan,
    writer: &mut Writer,
) {
    for (id, variable) in function.vars.iter() {
        if layouts.type_repr(variable.ty) == Repr::Void {
            continue;
        }
        if function.params.contains(&id)
            && parameter_is_loop_variable(function, id)
            && layouts.is_managed_value(variable.ty)
        {
            writer.decl("int", &owned_variable_name(id), Some("INT32_C(0)"));
            writer.line(&format!("(void)&{};", owned_variable_name(id)));
        }
        if !function.params.contains(&id) {
            if layouts.is_managed_value(variable.ty) {
                let initial = if layouts.is_managed_list(variable.ty) {
                    "NULL"
                } else {
                    "{0}"
                };
                writer.decl(
                    &layouts.c_type(variable.ty),
                    &variable_name(id),
                    Some(initial),
                );
                writer.decl("int", &owned_variable_name(id), Some("INT32_C(0)"));
                // A non-returning call can remove every ownership use of a
                // declared local, just as it can remove uses of the value itself.
                writer.line(&format!("(void)&{};", owned_variable_name(id)));
            } else {
                writer.decl(&layouts.c_type(variable.ty), &variable_name(id), None);
            }
        }
    }
    for (name, capacity) in &string_buffers.declarations {
        writer.line(&format!("char {name}[{capacity}];"));
    }
    for (variable, data) in function.vars.iter() {
        if layouts.type_repr(data.ty) == Repr::Void {
            continue;
        }
        writer.line(&format!("(void)&{};", variable_name(variable)));
    }
}

pub(in crate::backend::emit) fn emit_release_if_owned(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    variable: VarId,
    writer: &mut Writer,
) {
    let ty = function.vars[variable].ty;
    if !layouts.is_managed_value(ty) {
        return;
    }
    writer.begin_block(&format!(
        "if ({} != INT32_C(0))",
        owned_variable_name(variable)
    ));
    layouts.emit_release_value_expression(ty, &variable_name(variable), writer);
    writer.end_block();
    writer.assign(&owned_variable_name(variable), "INT32_C(0)");
}

pub(in crate::backend::emit) fn emit_retain_operand(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    operand: &Operand,
    writer: &mut Writer,
) {
    match operand {
        Operand::Var(variable) if layouts.is_managed_value(function.vars[*variable].ty) => {
            layouts.emit_retain_value_expression(
                function.vars[*variable].ty,
                &variable_name(*variable),
                writer,
            );
        }
        _ => {}
    }
}

// Ordinary parameters are borrowed. A parameter reused as a loop slot acquires
// ownership when the loop initializes it and must then participate in cleanup.
fn parameter_is_loop_variable(function: &Function, variable: VarId) -> bool {
    function.stmts.iter().any(
        |(_, statement)| matches!(statement, Stmt::Loop { vars, .. } if vars.contains(&variable)),
    )
}

pub(in crate::backend::emit) fn emit_owned_cleanup(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    except: Option<VarId>,
    writer: &mut Writer,
) {
    for (variable, data) in function.vars.iter() {
        if except == Some(variable)
            || (function.params.contains(&variable)
                && !parameter_is_loop_variable(function, variable))
            || !layouts.is_managed_value(data.ty)
        {
            continue;
        }
        emit_release_if_owned(layouts, function, variable, writer);
    }
    if layouts.runtime.has_cause_raise
        && layouts.runtime.has_unwind
        && !function.suspends
        && !function.constant
    {
        writer.line(
            "if (emela_current_fiber != NULL) emela_current_fiber->holds = emela_entry_holds;",
        );
    }
}

pub(in crate::backend::emit) fn signature(
    layouts: &TypeLayouts<'_>,
    function: &Function,
    name: &str,
    is_static: bool,
    takes_error_out: bool,
    takes_string_output: bool,
) -> String {
    let return_type = layouts.c_type(function.ret);
    let mut params: Vec<String> = function
        .params
        .iter()
        .map(|&variable| {
            let data = &function.vars[variable];
            format!("{} {}", layouts.c_type(data.ty), variable_name(variable))
        })
        .collect::<Vec<_>>();
    if takes_error_out {
        params.push("EmelaError *emela_error_out".to_owned());
    }
    if takes_string_output {
        params.push("char *emela_string_output".to_owned());
        params.push("uint16_t emela_string_capacity".to_owned());
    }
    let params = if params.is_empty() {
        "void".to_owned()
    } else {
        params.join(", ")
    };
    let linkage = if is_static { "static " } else { "" };
    format!("{linkage}{return_type} {name}({params})")
}

use super::layouts::LayoutEmission;
