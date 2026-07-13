use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::{
    parse_attributes, parse_parameter_list_with_docs, parse_visibility_or_default,
};
use crate::syntax::{
    Attribute, Block, Expression, Identifier, Parameter, Path, PrimitiveType, ReturnStatement,
    SpanInfo, Spanned, Statement, Type, Visibility,
};

use beskid_ast_derive::AstNode;

/// Method inside an `impl` block: receiver type, parameters, return type, and body.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MethodDefinition {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub receiver_type: Spanned<Type>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub parameters: Vec<Spanned<Parameter>>,
    #[ast(skip)]
    pub parameter_docs: Vec<Option<LeadingDocComment>>,
    #[ast(child)]
    pub return_type: Option<Spanned<Type>>,
    #[ast(child)]
    pub body: Spanned<Block>,
}

fn parse_path_segment(pair: Pair<Rule>) -> Result<Spanned<crate::syntax::PathSegment>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
    let mut type_args = Vec::new();
    if let Some(args) = inner.next() {
        for arg in args.into_inner() {
            type_args.push(Type::parse(arg)?);
        }
    }
    Ok(Spanned::new(
        crate::syntax::PathSegment { name, type_args },
        span,
    ))
}

impl Parsable for MethodDefinition {
    /// Always fails; use [`MethodDefinition::parse_with_receiver`] for `impl` methods.
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        Err(ParseError::unexpected_rule(
            pair,
            Some(Rule::ImplMethodDefinition),
        ))
    }
}

impl MethodDefinition {
    /// Parses an `impl` method given the already-parsed receiver type.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] on malformed input or if any parameter is named `self`.
    pub(crate) fn parse_with_receiver(
        pair: Pair<Rule>,
        receiver_type: Spanned<Type>,
    ) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let return_type = Some(Type::parse(
            inner.next().ok_or(ParseError::missing(Rule::BeskidType))?,
        )?);
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut parameters = Vec::new();
        let mut parameter_docs = Vec::new();
        let mut body = None;

        for item in inner {
            match item.as_rule() {
                Rule::ParameterList => {
                    let (parsed_parameters, parsed_docs) = parse_parameter_list_with_docs(item)?;
                    parameters = parsed_parameters;
                    parameter_docs = parsed_docs;
                }
                Rule::MethodBody => {
                    let method_body = item
                        .into_inner()
                        .next()
                        .ok_or(ParseError::missing(Rule::MethodBody))?;
                    body = Some(parse_method_body(method_body, span)?);
                }
                Rule::Block => body = Some(Block::parse(item)?),
                Rule::ExpressionBody => {
                    body = Some(parse_expression_body(item, span)?);
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        debug_assert_eq!(parameters.len(), parameter_docs.len());

        if let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.node.name.node.name == "self")
        {
            return Err(ParseError::forbidden_impl_self_parameter(
                parameter.node.name.span,
            ));
        }

        Ok(Spanned::new(
            Self {
                attributes,
                visibility,
                receiver_type,
                name,
                parameters,
                parameter_docs,
                return_type,
                body: body.ok_or(ParseError::missing(Rule::Block))?,
            },
            span,
        ))
    }
}

fn parse_method_body(pair: Pair<Rule>, span: SpanInfo) -> Result<Spanned<Block>, ParseError> {
    match pair.as_rule() {
        Rule::Block => Block::parse(pair),
        Rule::ExpressionBody => parse_expression_body(pair, span),
        _ => Err(ParseError::unexpected_rule(pair, Some(Rule::MethodBody))),
    }
}

fn parse_expression_body(pair: Pair<Rule>, span: SpanInfo) -> Result<Spanned<Block>, ParseError> {
    let expr_pair = pair
        .into_inner()
        .next()
        .ok_or(ParseError::missing(Rule::Expression))?;
    let expression = Expression::parse(expr_pair)?;
    Ok(block_from_expression(expression, span))
}

fn block_from_expression(expression: Spanned<Expression>, span: SpanInfo) -> Spanned<Block> {
    let return_stmt = Spanned::new(
        Statement::Return(Spanned::new(
            ReturnStatement {
                value: Some(expression),
            },
            span,
        )),
        span,
    );
    Spanned::new(
        Block {
            statements: vec![return_stmt],
        },
        span,
    )
}

/// Parses the `impl` receiver type (primitive, single segment, or full path).
pub(crate) fn parse_receiver_type(pair: Pair<Rule>) -> Result<Spanned<Type>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let first = if pair.as_rule() == Rule::ReceiverType {
        let mut inner = pair.into_inner();
        inner.next().ok_or(ParseError::missing(Rule::Identifier))?
    } else {
        pair
    };

    let node = match first.as_rule() {
        Rule::PrimitiveType => Type::Primitive(PrimitiveType::parse(first)?),
        Rule::PathSegment => {
            let segment = parse_path_segment(first)?;
            Type::Complex(Spanned::new(
                Path {
                    segments: vec![segment],
                },
                span,
            ))
        }
        Rule::Path => Type::Complex(Path::parse(first)?),
        _ => return Err(ParseError::unexpected_rule(first, Some(Rule::ReceiverType))),
    };

    Ok(Spanned::new(node, span))
}
