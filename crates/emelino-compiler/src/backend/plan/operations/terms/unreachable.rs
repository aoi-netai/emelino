pub(in crate::backend::plan) fn require_runtime(
    requirements: &mut crate::contract::runtime::RuntimeRequirements,
) {
    requirements.defects = true;
}
