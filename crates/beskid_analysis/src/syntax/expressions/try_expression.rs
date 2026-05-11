//! Postfix `?` on an expression (syntax sugar; semantics in analysis).

use crate::syntax::{Expression, Spanned};

use beskid_ast_derive::AstNode;

/// `expr?` — propagating try operator applied to an inner expression.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TryExpression {
    #[ast(child)]
    pub expr: Box<Spanned<Expression>>,
}
