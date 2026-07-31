//! Postfix `?` on an expression (syntax sugar; semantics in analysis).
//! Also supports `try { body } catch(err) { handler }` block form.

use crate::syntax::{BlockExpression, Expression, Identifier, Spanned};

use beskid_ast_derive::AstNode;

/// `expr?` — propagating try operator applied to an inner expression.
/// Also `try { body } catch(err) { handler }` block form.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TryExpression {
    #[ast(child)]
    pub expr: Box<Spanned<Expression>>,
    /// Error variable name for the `catch(err)` clause (block form only).
    #[ast(child)]
    pub error_variable: Option<Spanned<Identifier>>,
    /// Handler block for the `catch(err) { ... }` clause (block form only).
    #[ast(child)]
    pub catch_block: Option<Spanned<BlockExpression>>,
}
