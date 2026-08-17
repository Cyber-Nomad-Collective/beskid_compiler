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
        return Some(block_may_fall_through(&function.body.node));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(block_may_fall_through(&method.body.node));
    }
    if let Some(test) = node.of::<beskid_analysis::syntax::TestDefinition>() {
        return Some(statements_may_fall_through(&test.statements));
    }
    if let Some(block) = node.of::<beskid_analysis::syntax::Block>() {
        return Some(block_may_fall_through(block));
    }
    if let Some(statement) = node.of::<beskid_analysis::syntax::Statement>() {
        return Some(statement_may_fall_through(statement));
    }
    if let Some(if_statement) = node.of::<beskid_analysis::syntax::IfStatement>() {
        return Some(if_may_fall_through(if_statement));
    }
    if let Some(with_statement) = node.of::<beskid_analysis::syntax::WithStatement>() {
        return Some(block_may_fall_through(&with_statement.body.node));
    }
    if node.of::<beskid_analysis::syntax::ReturnStatement>().is_some()
        || node.of::<beskid_analysis::syntax::BreakStatement>().is_some()
        || node.of::<beskid_analysis::syntax::ContinueStatement>().is_some()
    {
        return Some(false);
    }
    None
}

pub(in crate::semantic_contract) fn block_may_fall_through(block: &beskid_analysis::syntax::Block) -> bool {
    statements_may_fall_through(&block.statements)
}

pub(in crate::semantic_contract) fn statements_may_fall_through(
    statements: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Statement>],
) -> bool {
    statements.iter().all(|statement| statement_may_fall_through(&statement.node))
}

pub(in crate::semantic_contract) fn statement_may_fall_through(statement: &beskid_analysis::syntax::Statement) -> bool {
    match statement {
        beskid_analysis::syntax::Statement::Return(_)
        | beskid_analysis::syntax::Statement::Break(_)
        | beskid_analysis::syntax::Statement::Continue(_) => false,
        beskid_analysis::syntax::Statement::If(if_statement) => if_may_fall_through(&if_statement.node),
        beskid_analysis::syntax::Statement::With(with_statement) => {
            block_may_fall_through(&with_statement.node.body.node)
        }
        beskid_analysis::syntax::Statement::Let(_)
        | beskid_analysis::syntax::Statement::While(_)
        | beskid_analysis::syntax::Statement::For(_)
        | beskid_analysis::syntax::Statement::Launch(_)
        | beskid_analysis::syntax::Statement::Expression(_) => true,
    }
}

pub(in crate::semantic_contract) fn if_may_fall_through(if_statement: &beskid_analysis::syntax::IfStatement) -> bool {
    if block_may_fall_through(&if_statement.then_block.node) {
        return true;
    }
    let Some(else_branch) = &if_statement.else_branch else {
        return true;
    };
    match &else_branch.node {
        beskid_analysis::syntax::ElseBranch::If(nested) => if_may_fall_through(&nested.node),
        beskid_analysis::syntax::ElseBranch::Block(block) => block_may_fall_through(&block.node),
    }
}
