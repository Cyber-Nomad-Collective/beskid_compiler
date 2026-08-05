use pest::iterators::Pair;
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::syntax::{Expression,SpanInfo,Spanned};
use beskid_ast_derive::AstNode;

#[derive(AstNode,Debug,Clone,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
pub struct ClifBlockExpression{pub body:String}
pub(crate) fn parse_clif_block(pair:Pair<Rule>)->Result<Spanned<Expression>,ParseError>{
let span=SpanInfo::from_span(&pair.as_span());
let mut inner=pair.into_inner();
let _kw=inner.next().ok_or(ParseError::missing(Rule::ClifKeyword))?;
let bp=inner.next().ok_or(ParseError::missing(Rule::Block))?;
let raw=bp.as_span().as_str();
let body=raw[1..raw.len().saturating_sub(1)].to_string();
Ok(Spanned::new(Expression::ClifBlock(Spanned::new(ClifBlockExpression{body},span)),span))
}