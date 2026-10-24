//! AST construction from Pest pairs: [`parsable::Parsable`] bridges grammar rules to [`crate::syntax::Spanned`] nodes.

pub mod diagnose;
pub mod error;
pub mod failure;
pub mod parsable;
pub mod reserved_keywords;
pub mod syntax_context;
