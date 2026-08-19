//! Built-in literal forms (numeric, string, char, bool).

use beskid_ast_derive::AstNode;

use crate::syntax::PrimitiveType;

/// Literal token; numeric and text forms keep raw source text where precision matters.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Literal {
    #[ast(skip)]
    Integer(String),
    #[ast(skip)]
    Float(String),
    #[ast(skip)]
    String(String),
    #[ast(skip)]
    Char(String),
    #[ast(skip)]
    Bool(bool),
    #[ast(skip)]
    Unit,
}

/// Strip the type suffix from an integer literal text (e.g. "42_i32" → "42").
pub fn integer_literal_magnitude(text: &str) -> &str {
    match text.find('_') {
        Some(pos) => &text[..pos],
        None => text,
    }
}

/// Determine the primitive type of an integer literal from its suffix or magnitude.
/// Literals with `_i64` suffix → I64, `_i32` suffix → I32, no suffix → I32 (default).
pub fn integer_literal_primitive_type(text: &str) -> PrimitiveType {
    if text.ends_with("_i64") {
        PrimitiveType::I64
    } else if text.ends_with("_i32") {
        PrimitiveType::I32
    } else {
        let magnitude = integer_literal_magnitude(text);
        match magnitude.parse::<i32>() {
            Ok(_) => PrimitiveType::I32,
            Err(_) => PrimitiveType::I64,
        }
    }
}

impl crate::parsing::parsable::Parsable for Literal {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let rule = pair.as_rule();
        let text = pair.as_str();

        let node = match rule {
            crate::parser::Rule::IntegerLiteral => Self::Integer(text.to_string()),
            crate::parser::Rule::FloatLiteral => Self::Float(text.to_string()),
            crate::parser::Rule::StringLiteral => Self::String(text.to_string()),
            crate::parser::Rule::CharLiteral => Self::Char(text.to_string()),
            crate::parser::Rule::Literal => {
                let mut inner = pair.clone().into_inner();
                if let Some(inner_pair) = inner.next() {
                    return Self::parse(inner_pair);
                }

                match text {
                    "true" => Self::Bool(true),
                    "false" => Self::Bool(false),
                    "()" => Self::Unit,
                    _ => {
                        return Err(crate::parsing::error::ParseError::unexpected_rule(
                            pair,
                            Some(crate::parser::Rule::Literal),
                        ));
                    }
                }
            }
            _ => {
                return Err(crate::parsing::error::ParseError::unexpected_rule(
                    pair,
                    Some(crate::parser::Rule::Literal),
                ));
            }
        };

        Ok(crate::syntax::Spanned::new(node, span))
    }
}
