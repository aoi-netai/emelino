//! crate::contract::ir::Stmt::Drop
use super::super::*;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    statement: &Stmt,
) -> Result<(), Vec<Error>> {
    let Stmt::Drop(variable) = statement else {
        unreachable!("Drop entry")
    };
    emit_release_if_owned(context.layouts, context.function, *variable, context.writer);
    Ok(())
}
