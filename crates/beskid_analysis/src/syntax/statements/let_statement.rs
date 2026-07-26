use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, Identifier, Literal, LiteralExpression, PrimitiveType, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Local binding with either an explicit initializer or a type-directed zero initializer.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LetStatement {
    #[ast(skip)]
    pub mutable: bool,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub type_annotation: Option<Spanned<Type>>,
    #[ast(child)]
    pub value: Spanned<Expression>,
}

impl Parsable for LetStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        if pair.as_rule() == Rule::LetStatement {
            let inner = pair.into_inner().next().ok_or(ParseError::missing(Rule::LetStatement))?;
            let parsed = Self::parse(inner)?;
            return Ok(Spanned::new(parsed.node, span));
        }

        let rule = pair.as_rule();
        let error_pair = pair.clone();
        let mut inner = pair.into_inner();
        let (mut mutable, mut name_pair, mut value_pair, mut type_annotation) = (false, None, None, None);

        match rule {
            Rule::TypedLetStatement => {
                let first = inner.next().ok_or(ParseError::missing(Rule::BeskidType))?;
                let type_pair = if first.as_rule() == Rule::MutKeyword {
                    mutable = true;
                    inner.next().ok_or(ParseError::missing(Rule::BeskidType))?
                } else {
                    first
                };
                type_annotation = Some(Type::parse(type_pair)?);
            }
            Rule::InferredLetStatement => {
                inner
                    .next()
                    .filter(|item| item.as_rule() == Rule::LetKeyword)
                    .ok_or(ParseError::missing(Rule::LetKeyword))?;
            }
            _ => {
                return Err(ParseError::unexpected_rule(error_pair, Some(Rule::LetStatement)));
            }
        }

        for item in inner {
            match item.as_rule() {
                Rule::LetKeyword => {}
                Rule::MutKeyword => {
                    if name_pair.is_some() {
                        return Err(ParseError::unexpected_rule(item, None));
                    }
                    mutable = true;
                }
                Rule::Identifier => name_pair = Some(item),
                Rule::Expression => value_pair = Some(item),
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }

        let name = Identifier::parse(name_pair.ok_or(ParseError::missing(Rule::Identifier))?)?;
        let value = match value_pair {
            Some(value) => Expression::parse(value)?,
            None => default_initializer(type_annotation.as_ref(), name.span)?,
        };

        Ok(Spanned::new(Self { mutable, name, type_annotation, value }, span))
    }
}

/// Materialize the language-defined default as an ordinary syntax literal so every later
/// semantic and ISLE phase follows the same path as an explicit initializer.
fn default_initializer(type_annotation: Option<&Spanned<Type>>, span: SpanInfo) -> Result<Spanned<Expression>, ParseError> {
    let primitive = type_annotation
        .and_then(|annotation| match &annotation.node {
            Type::Primitive(primitive) => Some(primitive.node),
            _ => None,
        })
        .ok_or(ParseError::missing(Rule::Expression))?;
    let literal = match primitive {
        PrimitiveType::Bool => Literal::Bool(false),
        PrimitiveType::I32 => Literal::Integer("0_i32".into()),
        PrimitiveType::I64 | PrimitiveType::Pointer | PrimitiveType::Word => Literal::Integer("0_i64".into()),
        PrimitiveType::U8 => Literal::Integer("0_u8".into()),
        PrimitiveType::F64 => Literal::Float("0.0".into()),
        PrimitiveType::Char => Literal::Char("'\\0'".into()),
        PrimitiveType::String => Literal::String("\"\"".into()),
        PrimitiveType::Unit | PrimitiveType::Never => return Err(ParseError::missing(Rule::Expression)),
    };
    let literal = Spanned::new(literal, span);
    let literal = Spanned::new(LiteralExpression { literal }, span);
    Ok(Spanned::new(Expression::Literal(literal), span))
}
