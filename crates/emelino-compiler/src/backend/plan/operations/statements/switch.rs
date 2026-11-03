use super::super::*;

pub(in crate::backend::plan) fn require_runtime(
    statement: &Stmt,
    requirements: &mut crate::contract::runtime::RuntimeRequirements,
) {
    requirements.defects |= matches!(
        statement,
        Stmt::Switch {
            on: SwitchOn::Tag(_),
            default: None,
            ..
        }
    );
}
