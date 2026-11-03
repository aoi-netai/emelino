use super::*;

pub(in crate::backend::plan) fn require_runtime(
    flow: &crate::contract::ir::Program,
    function: &Function,
    term: &Term,
    requirements: &mut crate::contract::runtime::RuntimeRequirements,
) {
    match term {
        Term::Defect(value) => defect::require_runtime(flow, function, value, requirements),
        Term::Unreachable => unreachable::require_runtime(requirements),
        Term::Yield(_)
        | Term::Return(_)
        | Term::Fail(_)
        | Term::Break { .. }
        | Term::Recur(_)
        | Term::Unwind => {}
    }
}

pub(in crate::backend::plan) mod r#break;
pub(in crate::backend::plan) mod defect;
pub(in crate::backend::plan) mod fail;
pub(in crate::backend::plan) mod recur;
pub(in crate::backend::plan) mod return_;
pub(in crate::backend::plan) mod unreachable;
pub(in crate::backend::plan) mod unwind;
pub(in crate::backend::plan) mod r#yield;
