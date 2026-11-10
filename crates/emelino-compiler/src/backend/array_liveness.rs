//! Backward liveness on structured Flow, including loop backedges and failure exits.
//! This is backend analysis; source hints and statement numbering are not ownership proofs.
use crate::contract::ir::*;
use std::collections::{HashMap, HashSet};
#[derive(Default)]
struct Node {
    uses: HashSet<VarId>,
    defs: HashSet<VarId>,
    next: Vec<usize>,
    statement: Option<StmtId>,
}
struct Graph<'a> {
    f: &'a Function,
    nodes: Vec<Node>,
    protected: HashSet<VarId>,
}
#[derive(Clone, Default)]
struct Edges {
    blocks: HashMap<LabelId, usize>,
    repeat: Option<(usize, Vec<VarId>)>,
}
fn operand(uses: &mut HashSet<VarId>, value: &Operand) {
    if let Operand::Var(v) = value {
        uses.insert(*v);
    }
}
impl Graph<'_> {
    fn add(&mut self, n: Node) -> usize {
        let id = self.nodes.len();
        self.nodes.push(n);
        id
    }
    fn sequence(&mut self, seq: SeqId, next: usize, edges: &Edges) -> usize {
        let s = &self.f.seqs[seq];
        let mut term = Node::default();
        match &s.term {
            Term::Yield(v) => {
                if let Some(v) = v {
                    operand(&mut term.uses, v)
                }
                term.next.push(next);
            }
            Term::Return(v) => {
                if let Some(v) = v {
                    operand(&mut term.uses, v)
                }
            }
            Term::Fail(v) | Term::Defect(v) => operand(&mut term.uses, v),
            Term::Break { label, value } => {
                if let Some(v) = value {
                    operand(&mut term.uses, v)
                }
                if let Some(&n) = edges.blocks.get(label) {
                    term.next.push(n);
                }
            }
            Term::Recur(vs) => {
                for v in vs {
                    operand(&mut term.uses, v)
                }
                if let Some((n, vars)) = &edges.repeat {
                    term.next.push(*n);
                    term.defs.extend(vars);
                }
            }
            Term::Unwind | Term::Unreachable => {}
        }
        let mut cursor = self.add(term);
        for &id in s.stmts.iter().rev() {
            let stmt = &self.f.stmts[id];
            let mut node = Node {
                statement: Some(id),
                ..Node::default()
            };
            if let Some(dst) = stmt.dst() {
                node.defs.insert(dst);
            }
            match stmt {
                Stmt::Let { value, .. } => {
                    value.for_each_var(|v| {
                        node.uses.insert(v);
                    });
                    node.next.push(cursor);
                    if let Value::Call {
                        on_fail: Some(OnFail::Break(label)),
                        ..
                    }
                    | Value::Fiber {
                        on_fail: Some(OnFail::Break(label)),
                        ..
                    } = value
                        && let Some(&n) = edges.blocks.get(label)
                    {
                        node.next.push(n);
                    }
                    // Legacy raw handler edges are deliberately conservative.
                    if matches!(
                        &value,
                        Value::Call {
                            on_fail: Some(OnFail::Handle { .. }),
                            ..
                        }
                    ) {
                        self.protected.extend(self.f.vars.iter().map(|(id, _)| id));
                    }
                }
                Stmt::If {
                    cond, then, else_, ..
                } => {
                    operand(&mut node.uses, cond);
                    node.next = vec![
                        self.sequence(*then, cursor, edges),
                        self.sequence(*else_, cursor, edges),
                    ];
                }
                Stmt::Switch {
                    on, cases, default, ..
                } => {
                    let (SwitchOn::Tag(v) | SwitchOn::Lit(v)) = on;
                    node.uses.insert(*v);
                    for c in cases {
                        node.next.push(self.sequence(c.body, cursor, edges));
                    }
                    node.next.push(
                        default
                            .map(|s| self.sequence(s, cursor, edges))
                            .unwrap_or(cursor),
                    );
                }
                Stmt::Block { label, body, .. } => {
                    let mut e = edges.clone();
                    e.blocks.insert(*label, cursor);
                    node.next.push(self.sequence(*body, cursor, &e));
                }
                Stmt::Loop {
                    vars, inits, body, ..
                } => {
                    for v in inits {
                        operand(&mut node.uses, v)
                    }
                    node.defs.extend(vars);
                    let head = self.add(Node::default());
                    let mut e = edges.clone();
                    e.repeat = Some((head, vars.clone()));
                    let body = self.sequence(*body, cursor, &e);
                    self.nodes[head].next.push(body);
                    node.next.push(head);
                }
                Stmt::Protect { body, cleanup, .. } => {
                    let old = self.nodes.len();
                    let clean = self.sequence(*cleanup, cursor, edges);
                    for n in &self.nodes[old..] {
                        self.protected.extend(&n.uses);
                    }
                    node.next.push(self.sequence(*body, clean, edges));
                }
                Stmt::Dup(v) => {
                    node.uses.insert(*v);
                    node.next.push(cursor);
                }
                Stmt::Drop(_) => {
                    node.next.push(cursor);
                }
            }
            cursor = self.add(node);
        }
        cursor
    }
}
pub(super) fn live_after(f: &Function) -> HashMap<StmtId, HashSet<VarId>> {
    let mut g = Graph {
        f,
        nodes: vec![Node::default()],
        protected: HashSet::new(),
    };
    if let Some(body) = f.body {
        g.sequence(body, 0, &Edges::default());
    }
    let mut live = vec![HashSet::new(); g.nodes.len()];
    loop {
        let mut changed = false;
        for (i, n) in g.nodes.iter().enumerate() {
            let mut next = g.protected.clone();
            for &s in &n.next {
                next.extend(&live[s]);
            }
            next.retain(|v| !n.defs.contains(v));
            next.extend(&n.uses);
            if next != live[i] {
                live[i] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut result = HashMap::new();
    for n in &g.nodes {
        if let Some(s) = n.statement {
            let out = result.entry(s).or_insert_with(HashSet::new);
            out.extend(&g.protected);
            for &s in &n.next {
                out.extend(&live[s]);
            }
        }
    }
    result
}
pub(super) fn can_update(f: &Function, dst: VarId, source: VarId) -> bool {
    if f.exported && (f.params.contains(&source) || loop_loan(f, source).is_some()) {
        return false;
    }
    let live = live_after(f);
    f.stmts
        .iter()
        .find(|(_, s)| s.dst() == Some(dst))
        .is_some_and(|(id, _)| live.get(&id).is_some_and(|out| !out.contains(&source)))
}

/// A live caller value must contribute an extra reference while a borrowed callee runs.
/// Guard locals participate in ordinary failure/interrupt cleanup, including suspend frames.
pub(super) fn protect_calls(f: &mut Function, types: &TypeTable) {
    if !f
        .vars
        .iter()
        .any(|(_, v)| matches!(types[v.ty], Type::Array(_)))
    {
        return;
    }
    let live = live_after(f);
    let mut defined = HashSet::new();
    for id in live.keys() {
        if let Some(dst) = f.stmts[*id].dst() {
            defined.insert(dst);
        }
        if let Stmt::Loop { vars, .. } = &f.stmts[*id] {
            defined.extend(vars);
        }
    }
    let locals: Vec<_> = f
        .vars
        .iter()
        .map(|(id, _)| id)
        .filter(|v| defined.contains(v) && !f.params.contains(v))
        .collect();
    let mut insert = HashMap::new();
    let ids: Vec<_> = f.stmts.iter().map(|(id, _)| id).collect();
    for id in ids {
        let Some(out) = live.get(&id) else { continue };
        let Stmt::Let { value, .. } = f.stmts[id].clone() else {
            continue;
        };
        if !matches!(
            &value,
            Value::Call { .. }
                | Value::Builtin {
                    op: Builtin::ArraySet,
                    ..
                }
        ) {
            continue;
        }
        let mut used = HashSet::new();
        value.for_each_var(|v| {
            used.insert(v);
        });
        let mut before = Vec::new();
        let mut after = Vec::new();
        // Reclaim dead aliases before testing uniqueness. Flags make drops of
        // not-yet-initialized or already-dropped locals harmless.
        for &v in &locals {
            if !out.contains(&v) && !used.contains(&v) {
                before.push(f.stmts.alloc(Stmt::Drop(v)));
            }
        }
        if let Value::Call { args, .. } = value.clone() {
            let mut guarded = HashSet::new();
            for a in args {
                if let Operand::Var(v) = a
                    && out.contains(&v)
                    && matches!(types[f.vars[v].ty], Type::Array(_))
                    && guarded.insert(v)
                {
                    let guard = f.vars.alloc(VarData {
                        hint: "array_borrow_guard".into(),
                        repr: Repr::Ref,
                        ty: f.vars[v].ty,
                    });
                    before.push(f.stmts.alloc(Stmt::Let {
                        dst: Some(guard),
                        value: Value::Coerce(v),
                        on_defect: None,
                        on_suspend: None,
                    }));
                    after.push(f.stmts.alloc(Stmt::Drop(guard)));
                }
            }
        }
        before.push(id);
        before.extend(after);
        insert.insert(id, before);
    }
    for (_, s) in f.seqs.iter_mut() {
        s.stmts = s
            .stmts
            .iter()
            .flat_map(|id| insert.get(id).cloned().unwrap_or_else(|| vec![*id]))
            .collect();
    }
}

/// A loop slot starts with an additional owned reference to a borrowed parameter.
/// Its original pointer remains borrowed for the whole call; identity is checked
/// at runtime so later arrays assigned to that slot do not inherit this loan.
pub(super) fn loop_loan(f: &Function, slot: VarId) -> Option<VarId> {
    if f.suspends {
        return None;
    }
    f.stmts.iter().find_map(|(_, statement)| {
        let Stmt::Loop { vars, inits, .. } = statement else {
            return None;
        };
        vars.iter().zip(inits).find_map(|(&v, init)| match init {
            Operand::Var(p) if v == slot && f.params.contains(p) => Some(*p),
            _ => None,
        })
    })
}
