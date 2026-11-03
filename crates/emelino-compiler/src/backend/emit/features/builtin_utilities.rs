//! Shared validation of a declared builtin signature. Operation behavior stays in its entry.
use super::*;

pub(super) enum ResultType {
    Exact(Type),
    Option(TypeId),
}

pub(super) fn prepare(
    context: &BuiltinContext<'_, '_>,
    dst: Option<VarId>,
    op: Builtin,
    args: &[Operand],
    arity: usize,
    result: ResultType,
    arguments_match: fn(&TypeLayouts<'_>, &Function, &[Operand]) -> bool,
) -> Result<(VarId, Vec<String>), Vec<Error>> {
    let layouts = context.layouts;
    let function = context.function;
    if args.len() != arity {
        return Err(invalid_flow_ir(
            &function.name,
            &format!(
                "builtin {op:?} has {} arguments, expected {arity}",
                args.len()
            ),
        ));
    }
    if !arguments_match(layouts, function, args) {
        return Err(invalid_flow_ir(
            &function.name,
            &format!("builtin {op:?} has an argument with the wrong type"),
        ));
    }
    let Some(dst) = dst else {
        return Err(invalid_flow_ir(
            &function.name,
            &format!("builtin {op:?} has no result destination"),
        ));
    };
    let matches = match result {
        ResultType::Exact(ty) => layouts.shapes.types[function.vars[dst].ty] == ty,
        ResultType::Option(ty) => option_payload_type(layouts, function.vars[dst].ty)
            .is_some_and(|payload| layouts.shapes.types[payload] == layouts.shapes.types[ty]),
    };
    if !matches {
        return Err(invalid_flow_ir(
            &function.name,
            &format!("builtin {op:?} has a result with the wrong type"),
        ));
    }
    let expressions = args
        .iter()
        .map(operand_expression)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|variant| unsupported(&function.name, variant))?;
    Ok((dst, expressions))
}
