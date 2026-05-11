//! [`Parsable`] maps a single Pest [`Pair`](pest::iterators::Pair) to a typed, spanned syntax node.

use pest::iterators::Pair;

use crate::parser::Rule;
use crate::syntax::Spanned;

use super::error::ParseError;

/// Implemented by syntax AST node types that deserialize from one grammar rule.
pub trait Parsable: Sized {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError>;
}
