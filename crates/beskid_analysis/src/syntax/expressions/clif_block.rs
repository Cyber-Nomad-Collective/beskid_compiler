use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Raw CLIF block expression (`clif { ... }`).
/// The body is an opaque string — no CLIF parsing happens at this stage.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClifBlockExpression {
    pub body: String,
}

pub(crate) fn parse_clif_block(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    // The full text of the ClifBlockExpression pair is: clif { ... }
    // Strip the leading "clif {" and trailing "}" to extract the body.
    let raw = pair.as_span().as_str();
    let body = raw
        .strip_prefix("clif")
        .unwrap_or(raw)
        .trim()
        .strip_prefix("{")
        .unwrap_or(raw)
        .trim_end()
        .strip_suffix("}")
        .unwrap_or(raw)
        .trim()
        .to_string();
    Ok(Spanned::new(Expression::ClifBlock(Spanned::new(ClifBlockExpression { body }, span)), span))
}
