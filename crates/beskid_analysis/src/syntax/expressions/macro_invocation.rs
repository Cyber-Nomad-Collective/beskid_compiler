use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::statements::Block;
use crate::syntax::{Expression, Identifier, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `name!(args)` / `name! { block }` macro invocation expression.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MacroInvocation {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub arguments: Vec<Spanned<Expression>>,
    #[ast(child)]
    pub block: Option<Spanned<Block>>,
}

impl Parsable for MacroInvocation {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut arguments = Vec::new();
        let mut block = None;
        for item in inner {
            match item.as_rule() {
                Rule::MacroArgumentList => {
                    for arg_pair in item.into_inner() {
                        if arg_pair.as_rule() == Rule::Expression {
                            arguments.push(Expression::parse(arg_pair)?);
                        }
                    }
                }
                Rule::MacroInvocationBlock => {
                    let block_pair = item
                        .into_inner()
                        .next()
                        .ok_or(ParseError::missing(Rule::Block))?;
                    block = Some(Block::parse(block_pair)?);
                }
                _ => {}
            }
        }

        Ok(Spanned::new(
            Self {
                name,
                arguments,
                block,
            },
            span,
        ))
    }
}
