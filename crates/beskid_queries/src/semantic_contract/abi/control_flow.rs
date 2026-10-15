//! Focused ABI semantic implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn control_flow_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ControlFlow> {
    with_node(db, syntax, key, |_program, _index, node| {
        control_flow_for_node(node).map(|may_fall_through| ControlFlow { may_fall_through })
    })
}

pub(in crate::semantic_contract) fn control_flow_for_node(
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<bool> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(function.body.node.may_fall_through());
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(method.body.node.may_fall_through());
    }
    if let Some(test) = node.of::<beskid_analysis::syntax::TestDefinition>() {
        return Some(beskid_analysis::syntax::statements_may_fall_through(&test.statements));
    }
    if let Some(block) = node.of::<beskid_analysis::syntax::Block>() {
        return Some(block.may_fall_through());
    }
    if let Some(statement) = node.of::<beskid_analysis::syntax::Statement>() {
        return Some(statement.may_fall_through());
    }
    if let Some(if_statement) = node.of::<beskid_analysis::syntax::IfStatement>() {
        return Some(if_statement.may_fall_through());
    }
    if let Some(with_statement) = node.of::<beskid_analysis::syntax::WithStatement>() {
        return Some(with_statement.body.node.may_fall_through());
    }
    if node.of::<beskid_analysis::syntax::ReturnStatement>().is_some()
        || node.of::<beskid_analysis::syntax::BreakStatement>().is_some()
        || node.of::<beskid_analysis::syntax::ContinueStatement>().is_some()
    {
        return Some(false);
    }
    None
}
