use super::types::HirPrimitiveType;

#[derive(
    beskid_ast_derive::PhaseFromAst, Debug, Clone, PartialEq, Eq, beskid_ast_derive::HirNode,
)]
#[ast(kind = "Literal")]
#[phase(source = "crate::syntax::Literal", phase = "crate::hir::HirPhase")]
pub enum HirLiteral {
    Integer(String),
    Float(String),
    String(String),
    Char(String),
    Bool(bool),
}

/// Default primitive for an integer literal token (suffix overrides; otherwise `i32`).
pub fn integer_literal_primitive_type(text: &str) -> HirPrimitiveType {
    if text.ends_with("_i64") {
        HirPrimitiveType::I64
    } else if text.ends_with("_u8") {
        HirPrimitiveType::U8
    } else {
        HirPrimitiveType::I32
    }
}

/// Numeric magnitude without an optional `_i32` / `_i64` / `_u8` suffix.
pub fn integer_literal_magnitude(text: &str) -> &str {
    text.strip_suffix("_i64")
        .or_else(|| text.strip_suffix("_i32"))
        .or_else(|| text.strip_suffix("_u8"))
        .unwrap_or(text)
}
