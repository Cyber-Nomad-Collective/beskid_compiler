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
    match text.rsplit_once('_') {
        Some((value, suffix)) if matches!(suffix, "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64") => value,
        _ => text,
    }
}

pub fn integer_literal_primitive_type(text: &str) -> PrimitiveType {
    for (suffix, primitive) in [
        ("_i8", PrimitiveType::I8),
        ("_i16", PrimitiveType::I16),
        ("_i32", PrimitiveType::I32),
        ("_i64", PrimitiveType::I64),
        ("_u8", PrimitiveType::U8),
        ("_u16", PrimitiveType::U16),
        ("_u32", PrimitiveType::U32),
        ("_u64", PrimitiveType::U64),
    ] {
        if text.ends_with(suffix) {
            return primitive;
        }
    }
    if text.starts_with("0x")
        && !integer_literal_fits_primitive(text, PrimitiveType::I64)
        && integer_literal_fits_primitive(text, PrimitiveType::Word)
    {
        return PrimitiveType::Word;
    }
    if integer_literal_fits_primitive(text, PrimitiveType::I32) { PrimitiveType::I32 } else { PrimitiveType::I64 }
}

/// Exact magnitude validation occurs before CLIF represents unsigned values as signed bits.
pub fn integer_literal_fits_primitive(text: &str, primitive: PrimitiveType) -> bool {
    let clean = integer_literal_magnitude(text).replace('_', "");
    let negative = clean.starts_with('-');
    let digits = clean.strip_prefix('-').unwrap_or(&clean);
    let magnitude = match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => digits.parse::<u64>().ok(),
    };
    let Some(value) = magnitude else { return false };
    let signed_limit = match primitive {
        PrimitiveType::I8 => Some(i8::MAX as u64),
        PrimitiveType::I16 => Some(i16::MAX as u64),
        PrimitiveType::I32 => Some(i32::MAX as u64),
        PrimitiveType::I64 => Some(i64::MAX as u64),
        _ => None,
    };
    if let Some(max) = signed_limit {
        return value <= max + u64::from(negative);
    }
    if negative {
        return false;
    }
    match primitive {
        PrimitiveType::U8 => value <= u8::MAX as u64,
        PrimitiveType::U16 => value <= u16::MAX as u64,
        PrimitiveType::U32 => value <= u32::MAX as u64,
        PrimitiveType::U64 | PrimitiveType::Word => true,
        _ => false,
    }
}

pub fn float_literal_primitive_type(text: &str) -> PrimitiveType {
    if text.ends_with("_f32") { PrimitiveType::F32 } else { PrimitiveType::F64 }
}

pub fn float_literal_magnitude(text: &str) -> &str {
    text.strip_suffix("_f32").or_else(|| text.strip_suffix("_f64")).unwrap_or(text)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_literal_suffix_selects_its_primitive_type() {
        assert_eq!(integer_literal_primitive_type("0_u8"), PrimitiveType::U8);
        assert_eq!(integer_literal_primitive_type("0_i32"), PrimitiveType::I32);
        assert_eq!(integer_literal_primitive_type("0_i64"), PrimitiveType::I64);
    }
}

#[cfg(test)]
mod v06_primitive_tests {
    use super::*;
    #[test]
    fn primitive_boundaries_reject_overflow_without_losing_u64_bits() {
        for (source, ty) in [
            ("-128_i8", PrimitiveType::I8),
            ("127_i8", PrimitiveType::I8),
            ("-32768_i16", PrimitiveType::I16),
            ("65535_u16", PrimitiveType::U16),
            ("18446744073709551615_u64", PrimitiveType::U64),
            ("18_446_744_073_709_551_615_u64", PrimitiveType::U64),
        ] {
            assert_eq!(integer_literal_primitive_type(source), ty);
            assert!(integer_literal_fits_primitive(source, ty));
        }
        for (source, ty) in [
            ("128_i8", PrimitiveType::I8),
            ("-129_i8", PrimitiveType::I8),
            ("-32769_i16", PrimitiveType::I16),
            ("65536_u16", PrimitiveType::U16),
            ("18446744073709551616_u64", PrimitiveType::U64),
            ("-1_u64", PrimitiveType::U64),
        ] {
            assert!(!integer_literal_fits_primitive(source, ty), "{source}");
        }
        assert_eq!(integer_literal_magnitude("18_446_u64"), "18_446");
        assert_eq!(float_literal_primitive_type("1.5_f32"), PrimitiveType::F32);
        assert_eq!(float_literal_magnitude("3.4028235e38_f32"), "3.4028235e38");
    }
}
