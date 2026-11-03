//! Reject broken references before the existing lowering passes index their arenas.
use super::invalid;
use crate::{Error, ir::*};
use la_arena::{Arena, Idx};
use std::collections::HashMap;

type Result<T = ()> = std::result::Result<T, Vec<Error>>;

fn index<T>(arena: &Arena<T>, id: Idx<T>, kind: &str) -> Result {
    if u32::from(id.into_raw()) as usize >= arena.len() {
        return Err(invalid(format!("{kind} ID is out of bounds")));
    }
    Ok(())
}
fn ty(p: &Program, id: TypeId) -> Result {
    if id.index() >= p.core.types.types.len() {
        return Err(invalid("type ID is out of bounds"));
    }
    Ok(())
}
fn type_children(t: &Type) -> Vec<TypeId> {
    match t {
        Type::Tuple(ts) | Type::Enum(_, ts) => ts.clone(),
        Type::List(t) | Type::Cell(t) => vec![*t],
        Type::Fn(f) => f
            .params
            .iter()
            .copied()
            .chain([f.ret])
            .chain(f.sets.into_iter().flat_map(|s| [s.errors, s.effects]))
            .collect(),
        Type::Set(s) => s.params.clone(),
        _ => vec![],
    }
}

/// Metadata caches are computed from the validated source tables, never decoded.
pub(super) fn prepare(p: &mut Program) -> Result {
    let canonical = TypeTable::default();
    if !p.core.types.types.starts_with(&canonical.types) {
        return Err(invalid("canonical builtin type IDs have been changed"));
    }
    let mut edges = Vec::with_capacity(p.core.types.types.len());
    for t in &p.core.types.types {
        for child in type_children(t) {
            ty(p, child)?;
        }
        match t {
            Type::Enum(id, args) => {
                index(&p.core.enums, *id, "enum")?;
                if p.core.enums[*id].params != args.len() {
                    return Err(invalid("enum type argument count differs from declaration"));
                }
            }
            Type::Handler(id) => index(&p.core.effects, *id, "effect")?,
            _ => {}
        }
        edges.push(type_children(t).iter().map(|id| id.index()).collect());
    }
    // Recursive data goes through EnumDef fields. Cycles within the TypeTable
    // itself cannot be constructed by the interner and make substitution diverge.
    let order = acyclic(&edges, "type")?;
    let types = &mut p.core.types;
    types.ids = HashMap::new();
    types.has_param = vec![false; types.types.len()];
    for i in order {
        types.has_param[i] = match &types.types[i] {
            Type::Param(_) => true,
            Type::Fn(f) if f.sets.is_some() => true,
            Type::Set(s) if !s.params.is_empty() => true,
            _ => edges[i].iter().any(|&child| types.has_param[child]),
        };
        // Keep IDs stable. Equivalent types may share an interning representative.
        types
            .ids
            .entry(types.types[i].clone())
            .or_insert(TypeId::from_index(i));
    }
    p.core.expanding_enums.clear();
    p.has_scopes = false;
    for (_, e) in p.core.enums.iter() {
        for v in &e.variants {
            if v.fields.len() != v.tys.len() {
                return Err(invalid("enum field names and types differ in length"));
            }
            for &t in &v.tys {
                ty(p, t)?;
            }
        }
    }
    for (_, e) in p.core.effects.iter() {
        for &t in &e.args {
            ty(p, t)?;
        }
        if let Some(id) = e.label_of {
            index(&p.core.effects, id, "effect label")?;
        }
        for op in &e.ops {
            ty(p, op.ret)?;
            for &t in &op.params {
                ty(p, t)?;
            }
        }
    }
    for &(id, f) in &p.error_shows {
        index(&p.core.enums, id, "error enum")?;
        index(&p.functions, f, "error display function")?;
    }
    for group in &p.const_function_groups {
        for &f in group {
            index(&p.functions, f, "const function")?;
        }
    }
    for (_, f) in p.functions.iter() {
        function(p, f)?;
    }
    for (_, f) in p.functions.iter_mut() {
        f.statement_positions.clear();
        f.sequence_positions.clear();
        f.active_statements.clear();
    }
    Ok(())
}

/// Postorder graph walk with a bounded native stack, used for all input arenas.
fn acyclic(edges: &[Vec<usize>], kind: &str) -> Result<Vec<usize>> {
    let mut state = vec![0u8; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for start in 0..edges.len() {
        if state[start] != 0 {
            continue;
        }
        let mut pending = vec![(start, false)];
        while let Some((node, exit)) = pending.pop() {
            if exit {
                state[node] = 2;
                order.push(node);
                continue;
            }
            match state[node] {
                2 => continue,
                1 => return Err(invalid(format!("cyclic {kind} graph"))),
                _ => {}
            }
            state[node] = 1;
            pending.push((node, true));
            for &child in edges[node].iter().rev() {
                if child >= edges.len() {
                    return Err(invalid(format!("{kind} ID is out of bounds")));
                }
                pending.push((child, false));
            }
        }
    }
    Ok(order)
}
fn variable(f: &Function, v: VarId) -> Result {
    index(&f.vars, v, "variable")
}
fn sequence(f: &Function, s: SeqId) -> Result {
    index(&f.seqs, s, "sequence")
}
fn operand(f: &Function, op: &Operand) -> Result {
    if let Operand::Var(v) = op {
        variable(f, *v)?;
    }
    Ok(())
}
fn operands(f: &Function, ops: &[Operand]) -> Result {
    for op in ops {
        operand(f, op)?;
    }
    Ok(())
}
fn on_fail(f: &Function, on: Option<OnFail>) -> Result {
    match on {
        Some(OnFail::Break(id)) => index(&f.labels, id, "label"),
        Some(OnFail::Handle { error, body }) => {
            variable(f, error)?;
            sequence(f, body)
        }
        _ => Ok(()),
    }
}
fn ctor(p: &Program, c: CtorRef) -> Result {
    index(&p.core.enums, c.enum_id, "constructor enum")?;
    if c.variant >= p.core.enums[c.enum_id].variants.len() {
        return Err(invalid("constructor variant is out of bounds"));
    }
    Ok(())
}
fn value(p: &Program, f: &Function, v: &Value) -> Result {
    match v {
        Value::Const(_) => {}
        Value::Prim { args, .. } | Value::Builtin { args, .. } | Value::Runtime { args, .. } => {
            operands(f, args)?
        }
        Value::Call {
            callee,
            args,
            on_fail: fail,
            ..
        } => {
            match callee {
                Callee::Direct(id) => index(&p.functions, *id, "callee")?,
                Callee::Indirect(id) => variable(f, *id)?,
                Callee::Op {
                    handler,
                    effect,
                    op,
                } => {
                    variable(f, *handler)?;
                    index(&p.core.effects, *effect, "callee effect")?;
                    if *op >= p.core.effects[*effect].ops.len() {
                        return Err(invalid("effect operation is out of bounds"));
                    }
                }
            }
            operands(f, args)?;
            on_fail(f, *fail)?;
        }
        Value::Make { ctor: c, args } => {
            ctor(p, *c)?;
            operands(f, args)?;
        }
        Value::Field { base, ctor: c, .. } => {
            variable(f, *base)?;
            ctor(p, *c)?;
        }
        Value::Tag(v)
        | Value::ListIsEmpty(v)
        | Value::ListHead(v)
        | Value::ListTail(v)
        | Value::Coerce(v) => variable(f, *v)?,
        Value::Tuple(args) => operands(f, args)?,
        Value::TupleGet { base, .. } | Value::EnvGet { env: base, .. } => variable(f, *base)?,
        Value::List { elems, tail } => {
            operands(f, elems)?;
            if let Some(op) = tail {
                operand(f, op)?;
            }
        }
        Value::FnRef(id) => index(&p.functions, *id, "function reference")?,
        Value::Closure(c) => {
            for &v in c.params.iter().chain(&c.captures) {
                variable(f, v)?;
            }
            sequence(f, c.body)?;
        }
        Value::MakeClosure { function, captures } => {
            index(&p.functions, *function, "closure function")?;
            for &v in captures {
                variable(f, v)?;
            }
        }
        Value::EvLookup { ev, effect } => {
            variable(f, *ev)?;
            index(&p.core.effects, *effect, "evidence effect")?;
        }
        Value::EvExtend { base, bindings } => {
            if let Some(v) = base {
                variable(f, *v)?;
            }
            for &(e, v) in bindings {
                index(&p.core.effects, e, "evidence effect")?;
                variable(f, v)?;
            }
        }
        Value::Handler { effect, ops, .. } => {
            index(&p.core.effects, *effect, "handler effect")?;
            for &v in ops {
                variable(f, v)?;
            }
        }
        Value::Fiber {
            op,
            args,
            on_fail: fail,
        } => {
            match op {
                FiberOp::Await { exit }
                | FiberOp::Attempt { exit, .. }
                | FiberOp::CauseKind { kind: exit } => index(&p.core.enums, *exit, "fiber enum")?,
                _ => {}
            }
            operands(f, args)?;
            on_fail(f, *fail)?;
        }
        Value::IsError {
            error,
            enum_id,
            error_type,
        }
        | Value::AsError {
            error,
            enum_id,
            error_type,
        } => {
            variable(f, *error)?;
            index(&p.core.enums, *enum_id, "error enum")?;
            ty(p, *error_type)?;
        }
    }
    Ok(())
}
fn statement(p: &Program, f: &Function, s: &Stmt) -> Result<Vec<SeqId>> {
    let mut children = vec![];
    if let Some(dst) = s.dst() {
        variable(f, dst)?;
    }
    match s {
        Stmt::Let {
            value: v,
            on_defect,
            on_suspend,
            ..
        } => {
            value(p, f, v)?;
            children.extend(*on_defect);
            if let Some(s) = on_suspend {
                for &v in &s.saved {
                    variable(f, v)?;
                }
            }
            match v {
                Value::Closure(c) => children.push(c.body),
                Value::Call {
                    on_fail: Some(OnFail::Handle { body, .. }),
                    ..
                }
                | Value::Fiber {
                    on_fail: Some(OnFail::Handle { body, .. }),
                    ..
                } => children.push(*body),
                _ => {}
            }
        }
        Stmt::If {
            cond, then, else_, ..
        } => {
            operand(f, cond)?;
            children.extend([*then, *else_]);
        }
        Stmt::Switch {
            on, cases, default, ..
        } => {
            let (SwitchOn::Tag(v) | SwitchOn::Lit(v)) = on;
            variable(f, *v)?;
            children.extend(cases.iter().map(|c| c.body));
            children.extend(*default);
        }
        Stmt::Loop {
            vars, inits, body, ..
        } => {
            for &v in vars {
                variable(f, v)?;
            }
            operands(f, inits)?;
            children.push(*body);
        }
        Stmt::Block { label, body, .. } => {
            index(&f.labels, *label, "label")?;
            children.push(*body);
        }
        Stmt::Protect { body, cleanup, .. } => children.extend([*body, *cleanup]),
        Stmt::Dup(v) | Stmt::Drop(v) => variable(f, *v)?,
    }
    for &s in &children {
        sequence(f, s)?;
    }
    Ok(children)
}
fn function(p: &Program, f: &Function) -> Result {
    ty(p, f.ret)?;
    for (_, v) in f.vars.iter() {
        ty(p, v.ty)?;
    }
    for &v in &f.params {
        variable(f, v)?;
    }
    if let Some(env) = &f.env {
        for &t in env {
            ty(p, t)?;
        }
    }
    if let Some(body) = f.body {
        sequence(f, body)?;
    }
    if let Some(id) = f.canonical_const {
        index(&p.functions, id, "canonical const")?;
    }
    let mut stmt_children = Vec::with_capacity(f.stmts.len());
    for (_, s) in f.stmts.iter() {
        stmt_children.push(statement(p, f, s)?);
    }
    let mut edges = Vec::with_capacity(f.seqs.len());
    for (_, seq) in f.seqs.iter() {
        let mut children = vec![];
        for &s in &seq.stmts {
            index(&f.stmts, s, "statement")?;
            children.extend(
                stmt_children[u32::from(s.into_raw()) as usize]
                    .iter()
                    .map(|s| u32::from(s.into_raw()) as usize),
            );
        }
        edges.push(children);
        match &seq.term {
            Term::Yield(op) | Term::Return(op) => {
                if let Some(op) = op {
                    operand(f, op)?;
                }
            }
            Term::Fail(op) | Term::Defect(op) => operand(f, op)?,
            Term::Break { label, value } => {
                index(&f.labels, *label, "label")?;
                if let Some(op) = value {
                    operand(f, op)?;
                }
            }
            Term::Recur(args) => operands(f, args)?,
            Term::Unwind | Term::Unreachable => {}
        }
    }
    acyclic(&edges, "sequence")?;
    Ok(())
}
