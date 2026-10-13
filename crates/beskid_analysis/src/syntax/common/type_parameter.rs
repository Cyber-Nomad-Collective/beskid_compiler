use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Attribute, Identifier, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// A declared type parameter (`T` in `Box<T>`) with the attributes written before it
/// (`<[A] T>`).
///
/// The node's span is the span of the parameter name, so span-keyed facts about a type
/// parameter do not depend on whether attributes are written; each attribute carries its own
/// span.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypeParameter {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    pub name: String,
}

impl TypeParameter {
    /// The parameter name as an identifier spanned like this parameter.
    pub fn identifier(parameter: &Spanned<TypeParameter>) -> Spanned<Identifier> {
        Spanned::new(Identifier { name: parameter.node.name.clone() }, parameter.span)
    }
}

/// Parses a `GenericParameters` pair. `TypeParameter` is a silent grammar rule, so its
/// attribute lists and identifiers arrive as a flat sequence.
pub(crate) fn parse_type_parameters(pair: Pair<Rule>) -> Result<Vec<Spanned<TypeParameter>>, ParseError> {
    if pair.as_rule() != Rule::GenericParameters {
        return Err(ParseError::unexpected_rule(pair, Some(Rule::GenericParameters)));
    }
    let mut parameters = Vec::new();
    let mut attributes = Vec::new();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::AttributeList => {
                attributes = item.into_inner().map(Attribute::parse).collect::<Result<Vec<_>, _>>()?;
            }
            Rule::Identifier => {
                let span = SpanInfo::from_span(&item.as_span());
                let name = item.as_str().to_string();
                parameters.push(Spanned::new(
                    TypeParameter { attributes: std::mem::take(&mut attributes), name },
                    span,
                ));
            }
            _ => return Err(ParseError::unexpected_rule(item, Some(Rule::GenericParameters))),
        }
    }
    Ok(parameters)
}
