//! Built-in primitive type keywords.

use beskid_ast_derive::AstNode;

/// Core primitive types supported in the surface language.
#[derive(AstNode, Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PrimitiveType {
    Bool,
    I32,
    I64,
    U32,
    U8,
    Pointer,
    Word,
    F64,
    Char,
    String,
    Unit,
    Never,
    I8,
    I16,
    U16,
    U64,
    F32,
}

impl crate::parsing::parsable::Parsable for PrimitiveType {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        if pair.as_rule() != crate::parser::Rule::PrimitiveType {
            return Err(crate::parsing::error::ParseError::unexpected_rule(
                pair,
                Some(crate::parser::Rule::PrimitiveType),
            ));
        }

        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let node = match pair.as_str() {
            "bool" => Self::Bool,
            "i8" => Self::I8,
            "i16" => Self::I16,
            "u16" => Self::U16,
            "u64" => Self::U64,
            "f32" => Self::F32,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "u32" => Self::U32,
            "u8" => Self::U8,
            "pointer" => Self::Pointer,
            "word" => Self::Word,
            "f64" => Self::F64,
            "char" => Self::Char,
            "string" => Self::String,
            "unit" => Self::Unit,
            "never" => Self::Never,
            _ => {
                return Err(crate::parsing::error::ParseError::unexpected_rule(
                    pair,
                    Some(crate::parser::Rule::PrimitiveType),
                ));
            }
        };

        Ok(crate::syntax::Spanned::new(node, span))
    }
}
