use crate::syntax::Spanned;

use super::common::{HirIdentifier, HirPath, HirVisibility};

/// Primitive types natively understood by the compiler and runtime.
///
/// Each variant maps to a concrete ABI representation: `I32`/`I64`/`F64`/`U8` for
/// arithmetic, `Bool` for logic, `Char` for Unicode scalars, `String` for heap
/// references, `Unit` for void returns, and `Never` for diverging expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, beskid_ast_derive::HirNode)]
#[ast(kind = "PrimitiveType")]
pub enum HirPrimitiveType {
    Bool,
    I32,
    I64,
    U8,
    F64,
    Char,
    String,
    Unit,
    Never,
}

impl HirPrimitiveType {
    pub fn bit_width(&self) -> u32 {
        match self {
            HirPrimitiveType::Bool => 1,
            HirPrimitiveType::U8 => 8,
            HirPrimitiveType::I32 => 32,
            HirPrimitiveType::I64 => 64,
            HirPrimitiveType::F64 => 64,
            HirPrimitiveType::Char => 32,
            HirPrimitiveType::String => 64,
            HirPrimitiveType::Unit => 0,
            HirPrimitiveType::Never => 0,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            HirPrimitiveType::I32 | HirPrimitiveType::I64 | HirPrimitiveType::U8
        )
    }
}

/// Type tree used in HIR declarations and expressions.
///
/// `Primitive` and `Complex` cover named/inline types, `Array` for `T[]`,
/// and `Function` for callable signatures.
#[derive(Debug, Clone, PartialEq, Eq, beskid_ast_derive::HirNode)]
#[ast(kind = "Type")]
pub enum HirType {
    #[ast(child)]
    Primitive(Spanned<HirPrimitiveType>),
    #[ast(child)]
    Complex(Spanned<HirPath>),
    #[ast(child)]
    Array(Box<Spanned<HirType>>),
    #[ast(children)]
    Function {
        return_type: Box<Spanned<HirType>>,
        parameters: Vec<Spanned<HirType>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, beskid_ast_derive::HirNode)]
#[ast(kind = "Field")]
pub struct HirField {
    #[ast(child)]
    pub visibility: Spanned<HirVisibility>,
    #[ast(skip)]
    pub kind: HirFieldKind,
    #[ast(skip)]
    pub event_capacity: Option<usize>,
    #[ast(skip)]
    pub inject_qualifier: Option<crate::syntax::InjectQualifier>,
    #[ast(child)]
    pub name: Spanned<HirIdentifier>,
    #[ast(child)]
    pub ty: Spanned<HirType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirFieldKind {
    Value,
    Event,
    Injected,
}

#[derive(Debug, Clone, PartialEq, Eq, beskid_ast_derive::HirNode)]
#[ast(kind = "Parameter")]
pub struct HirParameter {
    #[ast(skip)]
    pub mutable: bool,
    #[ast(child)]
    pub name: Spanned<HirIdentifier>,
    #[ast(child)]
    pub ty: Spanned<HirType>,
}
