use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::parse_visibility_or_default;
use crate::syntax::statements::Block;
use crate::syntax::{Identifier, SpanInfo, Spanned, Visibility};

use beskid_ast_derive::AstNode;

/// Fragment kind for a macro parameter (`block`, `expression`, …).
#[derive(AstNode, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroFragmentKind {
    Block,
    Expression,
    Statement,
    Type,
    Identifier,
    Literal,
    Pattern,
    Path,
    Item,
    Node,
}

impl MacroFragmentKind {
    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "block" => Self::Block,
            "expression" => Self::Expression,
            "statement" => Self::Statement,
            "type" => Self::Type,
            "identifier" => Self::Identifier,
            "literal" => Self::Literal,
            "pattern" => Self::Pattern,
            "path" => Self::Path,
            "item" => Self::Item,
            "node" => Self::Node,
            _ => return None,
        })
    }
}

/// One formal parameter in a `macro` definition.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct MacroParameter {
    #[ast(child)]
    pub kind: Spanned<MacroFragmentKind>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
}

/// `macro name (kind param, ...) { body }` module item.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct MacroDefinition {
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub parameters: Vec<Spanned<MacroParameter>>,
    #[ast(child)]
    pub body: Spanned<Block>,
}

impl Parsable for MacroParameter {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let kind_pair = inner
            .next()
            .ok_or(ParseError::missing(Rule::MacroFragmentKind))?;
        let kind_str = kind_pair.as_str();
        let kind_span = SpanInfo::from_span(&kind_pair.as_span());
        let kind = MacroFragmentKind::from_keyword(kind_str).ok_or_else(|| {
            ParseError::unexpected_rule(kind_pair.clone(), Some(Rule::MacroFragmentKind))
        })?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        Ok(Spanned::new(
            Self {
                kind: Spanned::new(kind, kind_span),
                name,
            },
            span,
        ))
    }
}

impl Parsable for MacroDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut parameters = Vec::new();
        let mut body = None;
        for item in inner {
            match item.as_rule() {
                Rule::MacroParameterList => {
                    for param_pair in item.into_inner() {
                        parameters.push(MacroParameter::parse(param_pair)?);
                    }
                }
                Rule::Block => {
                    body = Some(Block::parse(item)?);
                }
                _ => {}
            }
        }

        let body = body.ok_or(ParseError::missing(Rule::Block))?;
        Ok(Spanned::new(
            Self {
                visibility,
                name,
                parameters,
                body,
            },
            span,
        ))
    }
}
