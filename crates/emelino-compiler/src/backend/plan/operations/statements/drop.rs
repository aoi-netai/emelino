use super::super::*;

pub(in crate::backend::plan) fn lower(
    context: &mut LoweringContext<'_>,
    id: StmtId,
    _statement: &Stmt,
    next: usize,
) -> Result<usize, Vec<Error>> {
    // Raw Flow Drop is rejected before this point. Lowering introduces this
    // ownership action after an unused binding, including a suspending call.
    Ok(context.graph.add(Node::Statement(id, next)))
}
