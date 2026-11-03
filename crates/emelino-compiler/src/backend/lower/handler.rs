//! Resolve statically known handlers on the owned contract IR.
//! Adapted from the fixed upstream lower/handler.rs; see docs/migration-source.md.

use crate::contract::ir::{Operand, Stmt, SwitchOn, Term, VarId};

pub(super) fn statement_uses(statement: &Stmt, needle: VarId) -> bool {
    match statement {
        Stmt::Let {
            value, on_suspend, ..
        } => {
            let mut found = false;
            value.for_each_var(|variable| found |= variable == needle);
            found
                || on_suspend
                    .as_ref()
                    .is_some_and(|suspend| suspend.saved.contains(&needle))
        }
        Stmt::If { cond, .. } => operand_uses(cond, needle),
        Stmt::Switch { on, .. } => match on {
            SwitchOn::Tag(variable) | SwitchOn::Lit(variable) => *variable == needle,
        },
        Stmt::Loop { inits, .. } => inits.iter().any(|operand| operand_uses(operand, needle)),
        Stmt::Block { .. } | Stmt::Protect { .. } => false,
        Stmt::Dup(variable) | Stmt::Drop(variable) => *variable == needle,
    }
}

pub(super) fn term_uses(term: &Term, needle: VarId) -> bool {
    match term {
        Term::Yield(value) | Term::Return(value) => value
            .as_ref()
            .is_some_and(|value| operand_uses(value, needle)),
        Term::Fail(value) | Term::Defect(value) => operand_uses(value, needle),
        Term::Break { value, .. } => value
            .as_ref()
            .is_some_and(|value| operand_uses(value, needle)),
        Term::Recur(values) => values.iter().any(|value| operand_uses(value, needle)),
        Term::Unwind | Term::Unreachable => false,
    }
}

fn operand_uses(operand: &Operand, needle: VarId) -> bool {
    matches!(operand, Operand::Var(variable) if *variable == needle)
}

// Loop assignments and parameters can carry a different handler than a local
// definition with the same variable ID. Only fold single-assignment locals.
