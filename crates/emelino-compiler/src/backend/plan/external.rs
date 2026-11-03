use super::*;
use crate::ExternalResult;
use crate::contract::bindings::{ExternalBindingPlan as Binding, ExternalBindings as Bindings};

pub(super) fn validate_adapter_symbol(function: &Function, symbol: &str) -> Result<(), Vec<Error>> {
    if !is_c_identifier(symbol) || c::is_reserved_alias(symbol) {
        return Err(unsupported(
            &function.name,
            &format!("external adapter symbol `{symbol}` is invalid or reserved"),
        ));
    }
    Ok(())
}

fn symbol(function: &Function) -> Result<String, Vec<Error>> {
    let binding = function
        .external
        .as_ref()
        .and_then(|e| e.binding("c"))
        .ok_or_else(|| unsupported(&function.name, "external function without a C binding"))?;
    let symbol = crate::semantics::host_refs::symbol(&binding.host).to_owned();
    if !is_c_identifier(&symbol) {
        return Err(unsupported(
            &function.name,
            "external function with an invalid C symbol",
        ));
    }
    Ok(symbol)
}

pub(in crate::backend::plan) fn resolve(
    flow: &crate::contract::ir::Program,
    emitted: &HashSet<FnId>,
    layouts: &mut TypeLayouts<'_>,
    options: &Options,
    services: &HashMap<FnId, crate::contract::services::Operation>,
) -> Result<Bindings, Vec<Error>> {
    let mut bindings = Bindings::new();
    let mut declarations: HashMap<String, (Vec<String>, ExternalResult)> = HashMap::new();
    let generated: HashSet<_> = flow
        .functions
        .iter()
        .filter(|(id, _)| emitted.contains(id))
        .map(|(id, f)| function_name(id, f))
        .collect();
    for (id, f) in flow
        .functions
        .iter()
        .filter(|(id, f)| emitted.contains(id) && f.external.is_some() && !f.suspends)
    {
        if services.contains_key(&id) {
            bindings.insert(
                id,
                Binding {
                    symbol: function_name(id, f),
                    result: ExternalResult::Owned,
                    adapted: true,
                    target_owned: true,
                },
            );
            continue;
        }
        if let Some(kind) = f.derivation {
            bindings.insert(
                id,
                Binding {
                    symbol: function_name(id, f),
                    result: if kind == crate::DerivedOperation::Show {
                        ExternalResult::StringBuffer
                    } else {
                        ExternalResult::Owned
                    },
                    adapted: true,
                    target_owned: false,
                },
            );
            continue;
        }
        if f.infallible_cause_fail {
            bindings.insert(
                id,
                Binding {
                    symbol: validate_external(flow, f)?,
                    result: ExternalResult::Owned,
                    adapted: false,
                    target_owned: false,
                },
            );
            continue;
        }
        symbol(f)?;

        let binding = {
            if f.fails {
                return Err(unsupported(
                    &f.name,
                    "failing synchronous external needs an explicit Options::with_external_adapter mapping",
                ));
            }
            Binding {
                symbol: validate_external(flow, f)?,
                result: ExternalResult::Owned,
                adapted: false,
                target_owned: false,
            }
        };
        if !binding.adapted && generated.contains(&binding.symbol) {
            return Err(unsupported(
                &f.name,
                "external symbol conflicts with a generated function",
            ));
        }
        let mut abi = vec![layouts.c_type(f.ret)];
        abi.extend(f.params.iter().map(|&p| layouts.c_type(f.vars[p].ty)));
        if f.fails {
            abi.push("EmelaError *".into());
        }
        if binding.result == ExternalResult::StringBuffer {
            abi.extend(["char *".into(), "uint16_t".into()]);
        }
        match declarations.insert(binding.symbol.clone(), (abi.clone(), binding.result)) {
            Some(previous) if previous != (abi, binding.result) => {
                return Err(unsupported(
                    &f.name,
                    "external symbol is instantiated with incompatible signatures or result ownership",
                ));
            }
            _ => {}
        }
        if binding.adapted {
            for ty in f
                .params
                .iter()
                .map(|&p| f.vars[p].ty)
                .chain(std::iter::once(f.ret))
            {
                if contains_any(layouts, ty, &mut HashSet::new()) {
                    layouts.runtime.any_box_required = true;
                }
            }
        }
        bindings.insert(id, binding);
    }
    for (_id, f) in flow
        .functions
        .iter()
        .filter(|(id, f)| emitted.contains(id) && f.external.is_some() && f.suspends)
    {
        let symbol = suspend::external_symbol(f, options)?;
        let mut abi = vec!["void".into(), "EmelaTask".into()];
        abi.extend(f.params.iter().map(|&p| layouts.c_type(f.vars[p].ty)));
        match declarations.insert(symbol, (abi.clone(), ExternalResult::Owned)) {
            Some(previous) if previous != (abi, ExternalResult::Owned) => {
                return Err(unsupported(
                    &f.name,
                    "external symbol is instantiated with incompatible synchronous and task signatures",
                ));
            }
            _ => {}
        }
    }
    layouts.runtime.managed_error_payloads = layouts
        .runtime
        .error_types
        .iter()
        .any(|&(_, ty)| layouts.is_managed_value(ty));
    Ok(bindings)
}

/// Validate after suspension planning, when the generated wrapper and helper names are known.
pub(super) fn validate_generated_symbols(plan: &PlannedModule<'_>) -> Result<(), Vec<Error>> {
    let flow = &plan.input.lowered;
    let mut generated = HashSet::new();
    for (id, function) in flow
        .functions
        .iter()
        .filter(|(id, _)| plan.emitted_functions.contains(id))
    {
        let name = &plan.names[&id];
        let mut reserve = |symbol: String| {
            if !generated.insert(symbol.clone()) {
                return Err(unsupported(
                    &function.name,
                    &format!("generated C declaration `{symbol}` conflicts with another function"),
                ));
            }
            Ok(())
        };
        if function.suspends {
            reserve(format!("{name}_push"))?;
            if let Some(suspension) = plan.suspensions.get(&id) {
                for suffix in ["frame", "step", "drop"] {
                    reserve(format!("{name}_{suffix}"))?;
                }
                if function.exported {
                    reserve(format!("{name}_start"))?;
                }
                if suspension.has_failure {
                    reserve(format!("{name}_has_failure"))?;
                }
                if suspension.graph.max_depth != 0 {
                    reserve(format!("{name}_unwind"))?;
                }
            }
        } else if function.external.is_none() || plan.external_bindings[&id].adapted {
            reserve(name.clone())?;
            if function.exported && plan.public_names[&id] != *name {
                reserve(plan.public_names[&id].clone())?;
            }
        }
    }
    generated.extend(plan.aliases.iter().map(|(alias, _)| alias.clone()));
    for (id, function) in flow.functions.iter().filter(|(id, function)| {
        plan.emitted_functions.contains(id)
            && function.external.is_some()
            && !plan.services.contains_key(id)
            && function.derivation.is_none()
            && !function.infallible_cause_fail
    }) {
        let symbol = if function.suspends {
            &plan.names[&id]
        } else {
            &plan.external_bindings[&id].symbol
        };
        if generated.contains(symbol) {
            return Err(unsupported(
                &function.name,
                &format!("external symbol `{symbol}` conflicts with a generated declaration"),
            ));
        }
    }
    Ok(())
}

fn contains_any(layouts: &TypeLayouts<'_>, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    match &layouts.shapes.types[ty] {
        Type::Any => true,
        Type::List(t) | Type::Cell(t) => contains_any(layouts, *t, seen),
        _ => layouts.shapes.aggregate_ids.get(&ty).is_some_and(|&index| {
            match &layouts.shapes.aggregates[index].kind {
                AggregateKind::Tuple(fields) => {
                    fields.iter().any(|&t| contains_any(layouts, t, seen))
                }
                AggregateKind::Enum(variants) => variants
                    .iter()
                    .flatten()
                    .any(|&t| contains_any(layouts, t, seen)),
            }
        }),
    }
}

pub(in crate::backend::plan) fn needs_heap(
    function: &Function,
    binding: Option<&Binding>,
    layouts: &TypeLayouts<'_>,
    options: &Options,
) -> bool {
    binding.is_some_and(|b| {
        b.adapted
            && ((b.result == ExternalResult::Borrowed && layouts.is_managed_value(function.ret))
                || (options.heap_bytes() != 0
                    && std::iter::once(function.ret)
                        .chain(function.params.iter().map(|&p| function.vars[p].ty))
                        .any(|ty| layouts.is_managed_value(ty))))
    })
}

pub(in crate::backend::plan) fn extend_string_output(
    flow: &crate::contract::ir::Program,
    emitted: &HashSet<FnId>,
    bindings: &Bindings,
    layouts: &TypeLayouts<'_>,
    output: &mut HashSet<FnId>,
) {
    output.extend(
        bindings
            .iter()
            .filter_map(|(&id, b)| (b.result == ExternalResult::StringBuffer).then_some(id)),
    );
    loop {
        let mut changed = false;
        for (id, f) in flow.functions.iter() {
            if !emitted.contains(&id)
                || f.external.is_some()
                || f.suspends
                || f.constant
                || layouts.functions.owned_string_returns.contains(&f.name)
                || f.ret != TypeId::STRING
                || output.contains(&id)
            {
                continue;
            }
            if f . stmts . iter () . any (| (_ , s) | matches ! (s , Stmt :: Let { value : Value :: Call { callee : Callee :: Direct (target) , .. } , .. } if output . contains (target))) { changed |= output . insert (id) ; }
        }
        if !changed {
            break;
        }
    }
}
