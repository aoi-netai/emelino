//! Allocation-free lowering of locally invoked scalar closures.
//!
//! No function values cross a boundary: every use must be a synchronous call in
//! the defining function. Captures are copied at creation, including loop slots.
//! Escaping, aliased, managed-capture, failing and suspended closures keep the
//! existing environment ABI. Small pure factories are expanded only at such uses.
use super::*;

pub(super) fn is_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::Int64 | Type::Float | Type::Bool)
}

/// Conservatively reject every use other than invoking this exact local value.
/// In particular, do not infer escape behavior of external or higher-order calls.
pub(super) fn direct_calls(function: &Function, variable: VarId) -> Option<Vec<StmtId>> {
    if function.suspends || function.constant || function.params.contains(&variable) {
        return None;
    }
    let (statements, sequences) = function_positions(function);
    let mut definitions = 0;
    let mut calls = Vec::new();
    for (id, statement) in function.stmts.iter() {
        if !statements.contains_key(&id) {
            continue;
        }
        definitions += usize::from(statement.dst() == Some(variable));
        match statement {
            Stmt::Let {
                value, on_suspend, ..
            } => {
                if on_suspend
                    .as_ref()
                    .is_some_and(|s| s.saved.contains(&variable))
                {
                    return None;
                }
                if let Value::Call {
                    callee: Callee::Indirect(callee),
                    args,
                    suspends: false,
                    on_fail: None,
                } = value
                    && *callee == variable
                {
                    if args.contains(&Operand::Var(variable)) {
                        return None;
                    }
                    calls.push(id);
                } else {
                    let mut used = false;
                    value.for_each_var(|v| used |= v == variable);
                    if used {
                        return None;
                    }
                }
            }
            Stmt::If { cond, .. } if *cond == Operand::Var(variable) => return None,
            Stmt::Switch {
                on: SwitchOn::Tag(v) | SwitchOn::Lit(v),
                ..
            } if *v == variable => {
                return None;
            }
            Stmt::Loop { vars, inits, .. }
                if vars.contains(&variable) || inits.contains(&Operand::Var(variable)) =>
            {
                return None;
            }
            Stmt::Dup(v) | Stmt::Drop(v) if *v == variable => return None,
            _ => {}
        }
    }
    for &sequence in sequences.keys() {
        let used = match &function.seqs[sequence].term {
            Term::Return(value) | Term::Yield(value) | Term::Break { value, .. } => {
                *value == Some(Operand::Var(variable))
            }
            Term::Fail(value) | Term::Defect(value) => *value == Operand::Var(variable),
            Term::Recur(args) => args.contains(&Operand::Var(variable)),
            Term::Unwind | Term::Unreachable => false,
        };
        if used {
            return None;
        }
    }
    (definitions == 1 && !calls.is_empty()).then_some(calls)
}

pub(super) fn lift_direct(
    program: &mut Module,
    parent: FnId,
    statement: StmtId,
    closure: &Closure,
    mut lifted: Function,
    capture_locals: &HashMap<VarId, VarId>,
    calls: &[StmtId],
) {
    lifted
        .params
        .extend(closure.captures.iter().map(|v| capture_locals[v]));
    let target = program.functions.alloc(lifted);
    let function = &mut program.functions[parent];
    let mut snapshots = Vec::new();
    let mut captures = Vec::new();
    for &source in &closure.captures {
        let mut data = function.vars[source].clone();
        data.hint = format!("{}_capture", data.hint);
        let snapshot = function.vars.alloc(data);
        snapshots.push(function.stmts.alloc(Stmt::Let {
            dst: Some(snapshot),
            value: Value::Coerce(source),
            on_defect: None,
            on_suspend: None,
        }));
        captures.push(Operand::Var(snapshot));
    }
    for &call in calls {
        let Stmt::Let {
            value: Value::Call { callee, args, .. },
            ..
        } = &mut function.stmts[call]
        else {
            unreachable!("direct-use analysis selected a non-call");
        };
        *callee = Callee::Direct(target);
        args.extend(captures.iter().cloned());
    }
    // Remove the binding from executable sequences and its old closure body.
    // Unused arena slots must not keep a Closure for subsequent lower iterations.
    function.stmts[statement] = Stmt::Let {
        dst: None,
        value: Value::Const(Const::Unit),
        on_defect: None,
        on_suspend: None,
    };
    for (_, sequence) in function.seqs.iter_mut() {
        if sequence.stmts.contains(&statement) {
            sequence.stmts = sequence
                .stmts
                .iter()
                .flat_map(|&id| {
                    if id == statement {
                        snapshots.clone()
                    } else {
                        vec![id]
                    }
                })
                .collect();
        }
    }
}

pub(super) fn inline_scalar_factories(
    program: &mut Module,
    options: &crate::Options,
) -> Result<(), Vec<Error>> {
    // Bound code growth: include the nested closure body in this budget.
    let factories: HashMap<_, _> = program
        .functions
        .iter()
        .filter_map(|(id, f)| {
            if f.constant
                || f.fails
                || f.suspends
                || f.external.is_some()
                || f.stmts.len() > 16
                || f.params
                    .iter()
                    .any(|&p| !is_scalar(program.core.ty(f.vars[p].ty)))
            {
                return None;
            }
            let body = &f.seqs[f.body?];
            let [statement] = body.stmts.as_slice() else {
                return None;
            };
            let Stmt::Let {
                dst: Some(dst),
                value: Value::Closure(closure),
                on_defect: None,
                on_suspend: None,
            } = &f.stmts[*statement]
            else {
                return None;
            };
            if closure.suspends
                || body.term != Term::Return(Some(Operand::Var(*dst)))
                || closure.captures.iter().any(|v| !f.params.contains(v))
            {
                return None;
            }
            let Type::Fn(ft) = program.core.ty(f.vars[*dst].ty) else {
                return None;
            };
            (!ft.fails && !ft.suspends).then_some((id, (f.clone(), (**closure).clone())))
        })
        .collect();
    let reachable = runtime_reachable(program, options);
    let mut sites = Vec::new();
    for (id, function) in program.functions.iter() {
        if !reachable.contains(&id) {
            continue;
        }
        let (active, _) = function_positions(function);
        for (statement, stmt) in function.stmts.iter() {
            let Stmt::Let {
                dst: Some(dst),
                value:
                    Value::Call {
                        callee: Callee::Direct(target),
                        args,
                        suspends: false,
                        on_fail: None,
                    },
                on_defect: None,
                on_suspend: None,
            } = stmt
            else {
                continue;
            };
            if active.contains_key(&statement)
                && factories.contains_key(target)
                && direct_calls(function, *dst).is_some()
            {
                sites.push((id, statement, *target, args.clone()));
            }
        }
    }
    for (parent, statement, target, args) in sites {
        let (factory, closure) = &factories[&target];
        let (body, capture_locals) = lift_body(factory, closure, &[], &factory.name)?;
        let function = &mut program.functions[parent];
        let (variables, sequences) = import_body(function, &body)?;
        let mut initializers = Vec::new();
        let mut captures = Vec::new();
        for &capture in &closure.captures {
            let dst = variables[&capture_locals[&capture]];
            let argument = args[factory.params.iter().position(|p| *p == capture).unwrap()].clone();
            let value = match argument {
                Operand::Var(v) => Value::Coerce(v),
                Operand::Const(c) => Value::Const(c),
            };
            initializers.push(function.stmts.alloc(Stmt::Let {
                dst: Some(dst),
                value,
                on_defect: None,
                on_suspend: None,
            }));
            captures.push(dst);
        }
        let Stmt::Let { value, .. } = &mut function.stmts[statement] else {
            unreachable!()
        };
        *value = Value::Closure(Box::new(Closure {
            params: body.params.iter().map(|v| variables[v]).collect(),
            captures,
            body: sequences[&body.body.unwrap()],
            suspends: false,
        }));
        for (_, sequence) in function.seqs.iter_mut() {
            if let Some(index) = sequence.stmts.iter().position(|id| *id == statement) {
                sequence
                    .stmts
                    .splice(index..index, initializers.iter().copied());
            }
        }
    }
    Ok(())
}

type ImportedBody = (HashMap<VarId, VarId>, HashMap<SeqId, SeqId>);

fn import_body(target: &mut Function, source: &Function) -> Result<ImportedBody, Vec<Error>> {
    let variables = source
        .vars
        .iter()
        .map(|(v, data)| (v, target.vars.alloc(data.clone())))
        .collect();
    let labels = source
        .labels
        .iter()
        .map(|(v, data)| (v, target.labels.alloc(data.clone())))
        .collect();
    let sequences: HashMap<_, _> = source
        .seqs
        .iter()
        .map(|(v, _)| {
            (
                v,
                target.seqs.alloc(Seq {
                    stmts: Vec::new(),
                    term: Term::Unreachable,
                }),
            )
        })
        .collect();
    let mut statements = HashMap::new();
    for (id, statement) in source.stmts.iter() {
        let remapped = remap_statement(
            statement.clone(),
            &variables,
            &labels,
            &sequences,
            &target.name,
        )?;
        statements.insert(id, target.stmts.alloc(remapped));
    }
    for (id, sequence) in source.seqs.iter() {
        target.seqs[sequences[&id]] = Seq {
            stmts: sequence.stmts.iter().map(|v| statements[v]).collect(),
            term: remap_term(sequence.term.clone(), &variables, &labels, &target.name)?,
        };
    }
    Ok((variables, sequences))
}
