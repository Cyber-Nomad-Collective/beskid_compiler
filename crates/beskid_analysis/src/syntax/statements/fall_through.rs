//! Syntax-only control-flow fact: whether execution can continue past a statement sequence.
//!
//! This is the single authority for structured termination. Function bodies use it to prove
//! that every path returns, and a block expression that cannot fall through (for example a
//! `match` arm ending in `return`) produces no value and is typed `never`.

use super::{Block, ElseBranch, IfStatement, Statement};
use crate::syntax::Spanned;

impl Block {
    /// `false` when every path through the block ends in `return`, `break`, or `continue`.
    pub fn may_fall_through(&self) -> bool {
        statements_may_fall_through(&self.statements)
    }
}

impl Statement {
    /// `false` when the statement unconditionally transfers control out of its sequence.
    pub fn may_fall_through(&self) -> bool {
        match self {
            Statement::Return(_) | Statement::Break(_) | Statement::Continue(_) => false,
            Statement::If(if_statement) => if_statement.node.may_fall_through(),
            Statement::With(with_statement) => with_statement.node.body.node.may_fall_through(),
            Statement::Use(scoped) => scoped.node.body.as_ref().is_none_or(|body| body.node.may_fall_through()),
            Statement::Let(_)
            | Statement::While(_)
            | Statement::For(_)
            | Statement::Launch(_)
            | Statement::Expression(_) => true,
        }
    }
}

impl IfStatement {
    /// `false` only when the `then` block and every `else` branch cannot fall through.
    pub fn may_fall_through(&self) -> bool {
        if self.then_block.node.may_fall_through() {
            return true;
        }
        let Some(else_branch) = &self.else_branch else {
            return true;
        };
        match &else_branch.node {
            ElseBranch::If(nested) => nested.node.may_fall_through(),
            ElseBranch::Block(block) => block.node.may_fall_through(),
        }
    }
}

/// `false` when any statement of the sequence cannot fall through.
pub fn statements_may_fall_through(statements: &[Spanned<Statement>]) -> bool {
    statements.iter().all(|statement| statement.node.may_fall_through())
}
