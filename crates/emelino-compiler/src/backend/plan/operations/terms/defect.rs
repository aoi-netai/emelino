use super::super::*;

pub(in crate::backend::plan) fn require_runtime(
    flow: &crate::contract::ir::Program,
    function: &Function,
    value: &Operand,
    requirements: &mut crate::contract::runtime::RuntimeRequirements,
) {
    requirements.defects = true;
    if matches ! (value , Operand :: Var (var) if matches ! (flow . core . ty (function . vars [* var] . ty) , Type :: String))
    {
        requirements.require(crate::contract::runtime::RuntimeHelper::DynamicDefect);
    }
}
