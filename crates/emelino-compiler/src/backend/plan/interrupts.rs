//! Existing interrupt entry policy over the shared compiler analysis.
use super::*;
use crate::contract::lowered::ClosureMetadata;
use crate::contract::services::Operation;

mod targets;

fn gpio_registration(operation: &Operation, flow: &Program) -> bool {
    operation.peripheral == "gpio"
        && operation.name == "enable_interrupt"
        && operation.params.len() == 4
        && matches!(flow.core.types[operation.params[3]], Type::Fn(_))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_isr_entries(
    flow: &crate::contract::ir::Program,
    emitted: &HashSet<FnId>,
    heap_functions: &HashSet<FnId>,
    layouts: &TypeLayouts<'_>,
    names: &HashMap<FnId, String>,
    options: &Options,
    closures: &ClosureMetadata,
    services: &HashMap<FnId, Operation>,
) -> Result<Vec<String>, Vec<Error>> {
    let mut required = HashSet::new();
    for entry_name in options.isr_entries() {
        let Some((entry, _)) = flow.functions.iter().find(|(id, function)| {
            emitted.contains(id) && function.exported && function.name == *entry_name
        }) else {
            return Err(vec![Error {
                kind: ErrorKind::InvalidOptions,
                message: format!(
                    "ISR entry `{entry_name}` must name an emitted exported Emela function"
                ),
            }]);
        };
        validate_isr_function(
            flow,
            entry,
            entry_name,
            heap_functions,
            layouts,
            names,
            options,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut required,
            services,
        )?;
    }
    if services
        .values()
        .any(|operation| gpio_registration(operation, flow))
    {
        let analysis = targets::Analysis::new(flow, emitted, closures);
        let mut callbacks = HashSet::new();
        for call in &analysis.calls {
            let callees = analysis.callees(call);
            if !callees.functions.iter().any(|callee| {
                services
                    .get(callee)
                    .is_some_and(|operation| gpio_registration(operation, flow))
            }) {
                continue;
            }
            let candidates = analysis.argument(call, 3);
            if !candidates.is_resolved() {
                return Err(unsupported(
                    &flow.functions[call.caller].name,
                    "GPIO interrupt callback targets cannot be determined statically",
                ));
            }
            callbacks.extend(candidates.functions);
        }
        let mut callbacks: Vec<_> = callbacks.into_iter().collect();
        callbacks.sort_by_key(|id| flow.functions[*id].name.clone());
        for callback in callbacks {
            let entry_name = format!("GPIO callback {}", flow.functions[callback].name);
            validate_isr_function(
                flow,
                callback,
                &entry_name,
                heap_functions,
                layouts,
                names,
                options,
                &mut HashSet::new(),
                &mut HashSet::new(),
                &mut required,
                services,
            )?;
        }
    }
    let mut required: Vec<_> = required.into_iter().collect();
    required.sort();
    Ok(required)
}

// Keep the lowering context explicit across this emission boundary.
#[allow(clippy::too_many_arguments)]
fn validate_isr_function(
    flow: &crate::contract::ir::Program,
    id: FnId,
    entry_name: &str,
    heap_functions: &HashSet<FnId>,
    layouts: &TypeLayouts<'_>,
    names: &HashMap<FnId, String>,
    options: &Options,
    active: &mut HashSet<FnId>,
    complete: &mut HashSet<FnId>,
    required: &mut HashSet<String>,
    services: &HashMap<FnId, Operation>,
) -> Result<(), Vec<Error>> {
    if active.contains(&id) {
        return Err(unsupported(
            &flow.functions[id].name,
            &format!("recursive call reachable from ISR entry `{entry_name}`"),
        ));
    }
    if complete.contains(&id) {
        return Ok(());
    }
    let function = &flow.functions[id];
    if function.external.is_some() {
        let symbol = external::isr_symbol(flow, function, &names[&id], options, entry_name)?;
        if !options
            .isr_safe_externals()
            .iter()
            .any(|safe| safe == symbol)
        {
            return Err(unsupported(
                &function.name,
                &format!(
                    "external C function `{symbol}` reachable from ISR entry `{entry_name}` is not marked ISR-safe"
                ),
            ));
        }
        required.insert(symbol.to_owned());
    }
    if layouts.functions.runtime_consts.contains(&id) {
        return Err(unsupported(
            &function.name,
            &format!("runtime const initialization reachable from ISR entry `{entry_name}`"),
        ));
    }
    if function.suspends {
        return Err(unsupported(
            &function.name,
            &format!("suspending function reachable from ISR entry `{entry_name}`"),
        ));
    }
    if layouts
        .functions
        .closure_functions
        .get(&id)
        .is_some_and(|environment| {
            layouts.functions.closure_environments[*environment]
                .captures
                .iter()
                .any(|capture| layouts.is_managed_value(capture.ty))
        })
    {
        return Err(unsupported(
            &function.name,
            &format!("reference-managed closure capture reachable from ISR entry `{entry_name}`"),
        ));
    }
    let live_variables = live_variables(function);
    if heap_functions.contains(&id)
        || std::iter::once(function.ret)
            .chain(
                function
                    .vars
                    .iter()
                    .filter(|(id, _)| live_variables.contains(id))
                    .map(|(_, variable)| variable.ty),
            )
            .any(|ty| layouts.contains_heap_ref(ty, &mut HashSet::new()))
    {
        return Err(unsupported(
            &function.name,
            &format!("heap-backed value reachable from ISR entry `{entry_name}`"),
        ));
    }

    let reachable_statements = function.active_statements.clone();
    if function.stmts.iter().any(|(id, statement)| {
        reachable_statements.contains(&id)
            && matches!(
                statement,
                Stmt::Let {
                    value: Value::Runtime {
                        op: RuntimeOp::Hold | RuntimeOp::Unhold,
                        ..
                    },
                    ..
                }
            )
    }) {
        return Err(unsupported(
            &function.name,
            &format!("fiber interrupt control reachable from ISR entry `{entry_name}`"),
        ));
    }

    active.insert(id);
    for (_, statement) in function
        .stmts
        .iter()
        .filter(|(id, _)| function.active_statements.contains(id))
    {
        let Stmt::Let {
            value: Value::Call {
                callee, suspends, ..
            },
            ..
        } = statement
        else {
            continue;
        };
        if *suspends {
            active.remove(&id);
            return Err(unsupported(
                &function.name,
                &format!("suspending call reachable from ISR entry `{entry_name}`"),
            ));
        }
        match callee {
            Callee::Direct(callee_id) => {
                let callee = &flow.functions[*callee_id];
                if callee.suspends {
                    active.remove(&id);
                    return Err(unsupported(
                        &function.name,
                        &format!("suspending call reachable from ISR entry `{entry_name}`"),
                    ));
                }
                if callee.external.is_some() {
                    if let Some(operation) = services.get(callee_id) {
                        if operation.peripheral == "gpio"
                            && matches!(operation.name.as_str(), "read" | "write" | "toggle")
                        {
                            required.insert(format!("emela_gpio_{}", operation.name));
                            continue;
                        }
                        return Err(unsupported(
                            &function.name,
                            &format!(
                                "peripheral operation {}.{} is not ISR-safe (ISR entry `{entry_name}`)",
                                operation.peripheral, operation.name,
                            ),
                        ));
                    }
                    let symbol =
                        external::isr_symbol(flow, callee, &names[callee_id], options, entry_name)?;
                    if !options
                        .isr_safe_externals()
                        .iter()
                        .any(|safe| safe == symbol)
                    {
                        active.remove(&id);
                        return Err(unsupported(
                            &function.name,
                            &format!(
                                "external C function `{symbol}` reachable from ISR entry `{entry_name}` is not marked ISR-safe"
                            ),
                        ));
                    }
                    required.insert(symbol.to_owned());
                } else if !callee.constant {
                    validate_isr_function(
                        flow,
                        *callee_id,
                        entry_name,
                        heap_functions,
                        layouts,
                        names,
                        options,
                        active,
                        complete,
                        required,
                        services,
                    )?;
                }
            }
            Callee::Indirect(_) | Callee::Op { .. } => {
                active.remove(&id);
                return Err(unsupported(
                    &function.name,
                    &format!("dynamic call reachable from ISR entry `{entry_name}`"),
                ));
            }
        }
    }
    active.remove(&id);
    complete.insert(id);
    Ok(())
}

fn live_variables(function: &Function) -> HashSet<VarId> {
    let mut variables: HashSet<_> = function.params.iter().copied().collect();
    for (_, statement) in function
        .stmts
        .iter()
        .filter(|(id, _)| function.active_statements.contains(id))
    {
        variables.extend(statement.dst());
        match statement {
            Stmt::Let {
                value, on_suspend, ..
            } => {
                value.for_each_var(|variable| {
                    variables.insert(variable);
                });
                if let Some(suspend) = on_suspend {
                    variables.extend(&suspend.saved);
                }
            }
            Stmt::If {
                cond: Operand::Var(variable),
                ..
            }
            | Stmt::Switch {
                on: SwitchOn::Tag(variable) | SwitchOn::Lit(variable),
                ..
            }
            | Stmt::Dup(variable)
            | Stmt::Drop(variable) => {
                variables.insert(*variable);
            }
            Stmt::Loop { vars, inits, .. } => {
                variables.extend(vars);
                for initial in inits {
                    if let Operand::Var(var) = initial {
                        variables.insert(*var);
                    }
                }
            }
            _ => {}
        }
    }
    for (_, sequence) in function
        .seqs
        .iter()
        .filter(|(id, _)| function.sequence_positions.contains_key(id))
    {
        match &sequence.term {
            Term::Return(Some(Operand::Var(variable)))
            | Term::Yield(Some(Operand::Var(variable)))
            | Term::Break {
                value: Some(Operand::Var(variable)),
                ..
            }
            | Term::Fail(Operand::Var(variable))
            | Term::Defect(Operand::Var(variable)) => {
                variables.insert(*variable);
            }
            Term::Recur(args) => {
                for arg in args {
                    if let Operand::Var(var) = arg {
                        variables.insert(*var);
                    }
                }
            }
            _ => {}
        }
    }
    variables
}
