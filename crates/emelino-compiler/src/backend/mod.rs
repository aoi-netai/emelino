//! Pure orchestration over the independent backend contract.
pub(crate) mod emit;
pub(crate) mod lower;
pub(crate) mod plan;

pub(crate) fn compile_program(
    program: crate::contract::ir::Program,
    options: &crate::Options,
) -> Result<crate::Output, Vec<crate::Error>> {
    let lowered = lower::lower_program(program, options)?;
    let planned = plan::prepare(&lowered, options)?;
    emit::render(&planned)
}

/// Reuse the existing semantic fixtures to cover the persistence boundary too.
/// Invalid fixtures retain their original lowering diagnostic; every successful
/// fixture must also compile from its saved, source-independent input.
#[cfg(test)]
pub(crate) fn compile_saved_program(
    program: crate::ir::Program,
    options: &crate::Options,
) -> Result<crate::Output, Vec<crate::Error>> {
    let output = compile_program(program.clone(), options)?;
    let input = crate::BackendInput::new(program)?;
    let replay = crate::BackendInput::from_json(&input.to_json()?)?.compile(options)?;
    assert_eq!(
        output, replay,
        "saved input changed the generated artifacts"
    );
    Ok(replay)
}

mod array_liveness;
