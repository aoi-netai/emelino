//! Finite, whole-program points-to analysis for registered interrupt callbacks.
//!
//! Aggregate allocation sites are identities rather than recursively expanded
//! values, so recursive lists/records and loops still converge. Unknown foreign
//! values stay unknown when joined with known functions: one safe alternative
//! must never conceal an unchecked registration target.
use super::*;
use crate::contract::lowered::ClosureMetadata;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Targets {
    pub functions: HashSet<FnId>,
    objects: HashSet<(FnId, StmtId)>,
    pub unknown: bool,
}

impl Targets {
    fn merge(&mut self, other: &Self) -> bool {
        let previous = (self.functions.len(), self.objects.len(), self.unknown);
        self.functions.extend(&other.functions);
        self.objects.extend(&other.objects);
        self.unknown |= other.unknown;
        previous != (self.functions.len(), self.objects.len(), self.unknown)
    }

    pub fn is_resolved(&self) -> bool {
        !self.unknown && !self.functions.is_empty() && self.objects.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Node {
    Variable(FnId, VarId),
    Return(FnId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Field {
    Tuple(usize),
    Variant(EnumId, usize, usize),
    Element,
    Operation(EffectId, usize),
    Effect(EffectId),
}

#[derive(Default)]
struct Object {
    fields: HashMap<Field, Vec<Node>>,
    // A list tail or the base evidence table contributes the same projections.
    parents: Vec<Node>,
}

struct Projection {
    source: Node,
    field: Field,
    destination: Node,
}

pub(super) struct Call {
    pub caller: FnId,
    callee: Callee,
    arguments: Vec<Option<Node>>,
    destination: Option<Node>,
}

#[derive(Default)]
pub(super) struct Analysis {
    values: HashMap<Node, Targets>,
    objects: HashMap<(FnId, StmtId), Object>,
    copies: Vec<(Node, Node)>,
    projections: Vec<Projection>,
    pub calls: Vec<Call>,
}

impl Analysis {
    pub fn new(flow: &Program, emitted: &HashSet<FnId>, closures: &ClosureMetadata) -> Self {
        let mut analysis = Self::default();
        for (id, function) in flow.functions.iter().filter(|(id, _)| emitted.contains(id)) {
            if function.exported {
                for &parameter in &function.params {
                    analysis
                        .values
                        .entry(Node::Variable(id, parameter))
                        .or_default()
                        .unknown = true;
                }
            }
            if let Some(body) = function.body {
                analysis.sequence(id, function, body, None, &[], &HashMap::new());
            }
        }
        for (&target, &environment) in &closures.functions {
            let environment = &closures.environments[environment];
            for capture in &environment.captures {
                analysis.copies.push((
                    Node::Variable(environment.parent, capture.source),
                    Node::Variable(target, capture.local),
                ));
            }
        }
        loop {
            let mut updates = Vec::new();
            for &(source, destination) in &analysis.copies {
                updates.push((destination, analysis.value(source)));
            }
            for projection in &analysis.projections {
                updates.push((
                    projection.destination,
                    analysis.project(
                        &analysis.value(projection.source),
                        projection.field,
                        &mut HashSet::new(),
                    ),
                ));
            }
            for call in &analysis.calls {
                let targets = analysis.callees(call);
                if targets.unknown {
                    for argument in call.arguments.iter().flatten() {
                        analysis.foreign_arguments(flow, &analysis.value(*argument), &mut updates);
                    }
                    if let Some(destination) = call.destination {
                        updates.push((
                            destination,
                            Targets {
                                unknown: true,
                                ..Targets::default()
                            },
                        ));
                    }
                }
                for target in targets.functions {
                    let function = &flow.functions[target];
                    for (&parameter, argument) in function.params.iter().zip(&call.arguments) {
                        if let Some(argument) = argument {
                            updates.push((
                                Node::Variable(target, parameter),
                                analysis.value(*argument),
                            ));
                        }
                    }
                    if let Some(destination) = call.destination {
                        let result = if function.external.is_some() {
                            Targets {
                                unknown: true,
                                ..Targets::default()
                            }
                        } else {
                            analysis.value(Node::Return(target))
                        };
                        updates.push((destination, result));
                    }
                    if function.external.is_some() {
                        for argument in call.arguments.iter().flatten() {
                            analysis.foreign_arguments(
                                flow,
                                &analysis.value(*argument),
                                &mut updates,
                            );
                        }
                    }
                }
            }
            for (id, function) in flow
                .functions
                .iter()
                .filter(|(_, function)| function.exported)
            {
                if function.body.is_some() {
                    analysis.foreign_arguments(
                        flow,
                        &analysis.value(Node::Return(id)),
                        &mut updates,
                    );
                }
            }
            let mut changed = false;
            for (destination, value) in updates {
                changed |= analysis
                    .values
                    .entry(destination)
                    .or_default()
                    .merge(&value);
            }
            if !changed {
                break;
            }
        }
        analysis
    }

    pub fn callees(&self, call: &Call) -> Targets {
        match call.callee {
            Callee::Direct(target) => Targets {
                functions: HashSet::from([target]),
                ..Targets::default()
            },
            Callee::Indirect(variable) => self.value(Node::Variable(call.caller, variable)),
            Callee::Op {
                handler,
                effect,
                op,
            } => self.project(
                &self.value(Node::Variable(call.caller, handler)),
                Field::Operation(effect, op),
                &mut HashSet::new(),
            ),
        }
    }

    pub fn argument(&self, call: &Call, index: usize) -> Targets {
        call.arguments
            .get(index)
            .and_then(|node| *node)
            .map(|node| self.value(node))
            .unwrap_or_default()
    }

    fn value(&self, node: Node) -> Targets {
        self.values.get(&node).cloned().unwrap_or_default()
    }

    fn foreign_arguments(
        &self,
        flow: &Program,
        value: &Targets,
        updates: &mut Vec<(Node, Targets)>,
    ) {
        let mut pending = vec![value.clone()];
        let mut seen = HashSet::new();
        let mut seen_functions = HashSet::new();
        while let Some(value) = pending.pop() {
            for function in value.functions {
                if !seen_functions.insert(function) {
                    continue;
                }
                for &parameter in &flow.functions[function].params {
                    updates.push((
                        Node::Variable(function, parameter),
                        Targets {
                            unknown: true,
                            ..Targets::default()
                        },
                    ));
                }
                // Foreign code can call an escaped function and then call any
                // function it receives back, through arbitrarily many returns.
                pending.push(self.value(Node::Return(function)));
            }
            for site in value.objects {
                if !seen.insert(site) {
                    continue;
                }
                let object = &self.objects[&site];
                for node in object.fields.values().flatten().chain(&object.parents) {
                    pending.push(self.value(*node));
                }
            }
        }
    }

    fn project(
        &self,
        source: &Targets,
        field: Field,
        seen: &mut HashSet<(FnId, StmtId)>,
    ) -> Targets {
        let mut result = Targets {
            unknown: source.unknown,
            ..Targets::default()
        };
        for site in &source.objects {
            if !seen.insert(*site) {
                continue;
            }
            let object = &self.objects[site];
            if let Some(nodes) = object.fields.get(&field) {
                for node in nodes {
                    result.merge(&self.value(*node));
                }
                // A newer evidence binding shadows its base; list elements,
                // in contrast, include elements reached through every tail.
                if matches!(field, Field::Effect(_)) {
                    continue;
                }
            }
            for parent in &object.parents {
                result.merge(&self.project(&self.value(*parent), field, seen));
            }
        }
        result
    }

    fn copy_operand(&mut self, id: FnId, operand: &Operand, destination: Option<Node>) {
        if let (Operand::Var(source), Some(destination)) = (operand, destination) {
            self.copies.push((Node::Variable(id, *source), destination));
        }
    }

    fn sequence(
        &mut self,
        id: FnId,
        function: &Function,
        sequence: SeqId,
        destination: Option<Node>,
        loops: &[VarId],
        blocks: &HashMap<LabelId, Option<Node>>,
    ) {
        for &statement in &function.seqs[sequence].stmts {
            if !function.active_statements.contains(&statement) {
                continue;
            }
            let node = |variable| Node::Variable(id, variable);
            match &function.stmts[statement] {
                Stmt::Let {
                    dst,
                    value,
                    on_defect,
                    ..
                } => {
                    self.expression(id, statement, dst.map(node), value);
                    if let Some(body) = on_defect {
                        self.sequence(id, function, *body, dst.map(node), loops, blocks);
                    }
                    if let Value::Call {
                        on_fail: Some(OnFail::Handle { error, body }),
                        ..
                    }
                    | Value::Fiber {
                        on_fail: Some(OnFail::Handle { error, body }),
                        ..
                    } = value
                    {
                        self.values.entry(node(*error)).or_default().unknown = true;
                        self.sequence(id, function, *body, dst.map(node), loops, blocks);
                    }
                }
                Stmt::If {
                    dst, then, else_, ..
                } => {
                    self.sequence(id, function, *then, dst.map(node), loops, blocks);
                    self.sequence(id, function, *else_, dst.map(node), loops, blocks);
                }
                Stmt::Switch {
                    dst,
                    cases,
                    default,
                    ..
                } => {
                    for body in cases.iter().map(|case| case.body).chain(*default) {
                        self.sequence(id, function, body, dst.map(node), loops, blocks);
                    }
                }
                Stmt::Loop {
                    dst,
                    vars,
                    inits,
                    body,
                } => {
                    for (&variable, initial) in vars.iter().zip(inits) {
                        self.copy_operand(id, initial, Some(node(variable)));
                    }
                    self.sequence(id, function, *body, dst.map(node), vars, blocks);
                }
                Stmt::Block { dst, label, body } => {
                    let mut blocks = blocks.clone();
                    blocks.insert(*label, dst.map(node));
                    self.sequence(id, function, *body, dst.map(node), loops, &blocks);
                }
                Stmt::Protect { dst, body, cleanup } => {
                    self.sequence(id, function, *body, dst.map(node), loops, blocks);
                    self.sequence(id, function, *cleanup, None, loops, blocks);
                }
                Stmt::Dup(_) | Stmt::Drop(_) => {}
            }
        }
        match &function.seqs[sequence].term {
            Term::Yield(Some(value)) => self.copy_operand(id, value, destination),
            Term::Return(Some(value)) => self.copy_operand(id, value, Some(Node::Return(id))),
            Term::Break {
                label,
                value: Some(value),
            } => {
                self.copy_operand(id, value, blocks.get(label).copied().flatten());
            }
            Term::Recur(values) => {
                for (&variable, value) in loops.iter().zip(values) {
                    self.copy_operand(id, value, Some(Node::Variable(id, variable)));
                }
            }
            _ => {}
        }
    }

    fn expression(
        &mut self,
        id: FnId,
        statement: StmtId,
        destination: Option<Node>,
        value: &Value,
    ) {
        let node = |variable| Node::Variable(id, variable);
        let operand_node = |operand: &Operand| match operand {
            Operand::Var(variable) => Some(node(*variable)),
            Operand::Const(_) => None,
        };
        if let Value::Call { callee, args, .. } = value {
            self.calls.push(Call {
                caller: id,
                callee: callee.clone(),
                arguments: args.iter().map(operand_node).collect(),
                destination,
            });
            return;
        }
        let Some(destination) = destination else {
            return;
        };
        let mut object = Object::default();
        let mut add_field = |field, operand: &Operand| {
            if let Some(source) = operand_node(operand) {
                object.fields.entry(field).or_default().push(source);
            }
        };
        let projection = match value {
            Value::FnRef(function) => {
                self.values
                    .entry(destination)
                    .or_default()
                    .functions
                    .insert(*function);
                return;
            }
            Value::Coerce(source) | Value::ListTail(source) => {
                // A list tail can select any element of that list, which is a
                // conservative finite approximation independent of list length.
                self.copies.push((node(*source), destination));
                return;
            }
            Value::Tuple(values) => {
                for (index, value) in values.iter().enumerate() {
                    add_field(Field::Tuple(index), value);
                }
                None
            }
            Value::Make { ctor, args } => {
                for (index, value) in args.iter().enumerate() {
                    add_field(Field::Variant(ctor.enum_id, ctor.variant, index), value);
                }
                None
            }
            Value::List { elems, tail } => {
                for value in elems {
                    add_field(Field::Element, value);
                }
                if let Some(tail) = tail.as_ref().and_then(operand_node) {
                    object.parents.push(tail);
                }
                None
            }
            Value::Handler { effect, ops, .. } => {
                for (index, &operation) in ops.iter().enumerate() {
                    object
                        .fields
                        .insert(Field::Operation(*effect, index), vec![node(operation)]);
                }
                None
            }
            Value::EvExtend { base, bindings } => {
                for &(effect, handler) in bindings {
                    object
                        .fields
                        .insert(Field::Effect(effect), vec![node(handler)]);
                }
                if let Some(base) = base {
                    object.parents.push(node(*base));
                }
                None
            }
            Value::TupleGet { base, index } => Some((node(*base), Field::Tuple(*index))),
            Value::Field { base, ctor, index } => Some((
                node(*base),
                Field::Variant(ctor.enum_id, ctor.variant, *index),
            )),
            Value::ListHead(base) => Some((node(*base), Field::Element)),
            Value::EvLookup { ev, effect } => Some((node(*ev), Field::Effect(*effect))),
            Value::Const(_)
            | Value::Prim { .. }
            | Value::Tag(_)
            | Value::ListIsEmpty(_)
            | Value::IsError { .. } => return,
            _ => {
                self.values.entry(destination).or_default().unknown = true;
                return;
            }
        };
        if let Some((source, field)) = projection {
            self.projections.push(Projection {
                source,
                field,
                destination,
            });
        } else {
            self.objects.insert((id, statement), object);
            self.values
                .entry(destination)
                .or_default()
                .objects
                .insert((id, statement));
        }
    }
}
