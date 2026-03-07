use crate::syntax::{Expression, Spanned};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TryExpression {
    #[ast(child)]
    pub expr: Box<Spanned<Expression>>,
}
