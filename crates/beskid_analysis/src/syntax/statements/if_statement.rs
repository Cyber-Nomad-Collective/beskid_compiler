use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Block, Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `else` branch of an `if` statement: either another `if` chain or a block.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[ast(kind = "ElseBranch")]
pub enum ElseBranch {
    #[ast(child)]
    If(Box<Spanned<IfStatement>>),
    #[ast(child)]
    Block(Spanned<Block>),
}

/// Conditional with mandatory then-block and optional `else` branch.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IfStatement {
    #[ast(child)]
    pub condition: Spanned<Expression>,
    #[ast(child)]
    pub then_block: Spanned<Block>,
    #[ast(child)]
    pub else_branch: Option<Spanned<ElseBranch>>,
}

impl Parsable for ElseBranch {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        inner
            .next()
            .filter(|item| item.as_rule() == Rule::ElseKeyword)
            .ok_or(ParseError::missing(Rule::ElseKeyword))?;
        let branch = inner.next().ok_or(ParseError::missing(Rule::IfStatement))?;
        let node = match branch.as_rule() {
            Rule::IfStatement => Self::If(Box::new(IfStatement::parse(branch)?)),
            Rule::Block => Self::Block(Block::parse(branch)?),
            _ => return Err(ParseError::unexpected_rule(branch, None)),
        };
        Ok(Spanned::new(node, span))
    }
}

impl Parsable for IfStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let condition =
            Expression::parse(inner.next().ok_or(ParseError::missing(Rule::Expression))?)?;
        let then_block = Block::parse(inner.next().ok_or(ParseError::missing(Rule::Block))?)?;
        let else_branch = inner.next().map(ElseBranch::parse).transpose()?;

        Ok(Spanned::new(
            Self {
                condition,
                then_block,
                else_branch,
            },
            span,
        ))
    }
}
