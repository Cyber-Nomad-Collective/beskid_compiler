//! HIR type checker: [`context::TypeContext`] walks items and expressions, filling [`context::TypeResult`].

pub mod context;
pub mod expressions;
pub mod helpers;
pub mod items;
pub mod iterable;
pub mod spawn;
pub mod statements;
pub mod types;
