use crate::hir::{
    HirBinaryOp, HirEnumPath, HirField, HirFieldKind, HirIdentifier, HirParameter, HirPath, HirPathSegment,
    HirPrimitiveType, HirRangeExpression, HirType, HirUnaryOp, HirVisibility,
};
use crate::syntax::{self, Spanned};

use super::Lowerable;
use super::items::lower_attributes;

impl Lowerable for Spanned<syntax::Type> {
    type Output = Spanned<HirType>;

    fn lower(&self) -> Self::Output {
        let node = match &self.node {
            syntax::Type::Primitive(primitive) => HirType::Primitive(primitive.lower()),
            syntax::Type::Complex(path) => HirType::Complex(path.lower()),
            syntax::Type::Array(inner) => HirType::Array(Box::new(inner.lower())),
            syntax::Type::Function { return_type, parameters } => HirType::Function {
                return_type: Box::new(return_type.lower()),
                parameters: parameters.iter().map(Lowerable::lower).collect(),
            },
        };
        Spanned::new(node, self.span)
    }
}

impl Lowerable for Spanned<syntax::Field> {
    type Output = Spanned<HirField>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirField {
                attributes: lower_attributes(&self.node.attributes),
                visibility: self.node.visibility.lower(),
                kind: match self.node.kind {
                    syntax::FieldKind::Value => HirFieldKind::Value,
                    syntax::FieldKind::Event => HirFieldKind::Event,
                    syntax::FieldKind::Injected => HirFieldKind::Injected,
                },
                event_capacity: self.node.event_capacity,
                inject_qualifier: self.node.inject_qualifier,
                name: self.node.name.lower(),
                ty: self.node.ty.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::Parameter> {
    type Output = Spanned<HirParameter>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirParameter { mutable: self.node.mutable, name: self.node.name.lower(), ty: self.node.ty.lower() },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::RangeExpression> {
    type Output = Spanned<HirRangeExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(HirRangeExpression { start: self.node.start.lower(), end: self.node.end.lower() }, self.span)
    }
}

impl Lowerable for Spanned<syntax::Visibility> {
    type Output = Spanned<HirVisibility>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            match self.node {
                syntax::Visibility::Public => HirVisibility::Public,
                syntax::Visibility::Private => HirVisibility::Private,
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::Identifier> {
    type Output = Spanned<HirIdentifier>;

    fn lower(&self) -> Self::Output {
        Spanned::new(HirIdentifier { name: self.node.name.clone() }, self.span)
    }
}

impl Lowerable for Spanned<syntax::Path> {
    type Output = Spanned<HirPath>;

    fn lower(&self) -> Self::Output {
        Spanned::new(HirPath { segments: self.node.segments.iter().map(Lowerable::lower).collect() }, self.span)
    }
}

impl Lowerable for Spanned<syntax::PathSegment> {
    type Output = Spanned<HirPathSegment>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirPathSegment {
                name: self.node.name.lower(),
                type_args: self.node.type_args.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::EnumPath> {
    type Output = Spanned<HirEnumPath>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirEnumPath { type_path: self.node.type_path.lower(), variant: self.node.variant.lower() },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::PrimitiveType> {
    type Output = Spanned<HirPrimitiveType>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            match self.node {
                syntax::PrimitiveType::Bool => HirPrimitiveType::Bool,
                syntax::PrimitiveType::I32 => HirPrimitiveType::I32,
                syntax::PrimitiveType::I64 => HirPrimitiveType::I64,
                syntax::PrimitiveType::U8 => HirPrimitiveType::U8,
                syntax::PrimitiveType::Pointer => HirPrimitiveType::Word,
                syntax::PrimitiveType::Word => HirPrimitiveType::Word,
                syntax::PrimitiveType::F64 => HirPrimitiveType::F64,
                syntax::PrimitiveType::Char => HirPrimitiveType::Char,
                syntax::PrimitiveType::String => HirPrimitiveType::String,
                syntax::PrimitiveType::Unit => HirPrimitiveType::Unit,
                syntax::PrimitiveType::Never => HirPrimitiveType::Never,
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::BinaryOp> {
    type Output = Spanned<HirBinaryOp>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            match self.node {
                syntax::BinaryOp::Or => HirBinaryOp::Or,
                syntax::BinaryOp::And => HirBinaryOp::And,
                syntax::BinaryOp::IdentityEq => HirBinaryOp::IdentityEq,
                syntax::BinaryOp::IdentityNotEq => HirBinaryOp::IdentityNotEq,
                syntax::BinaryOp::Eq => HirBinaryOp::Eq,
                syntax::BinaryOp::NotEq => HirBinaryOp::NotEq,
                syntax::BinaryOp::Lt => HirBinaryOp::Lt,
                syntax::BinaryOp::Lte => HirBinaryOp::Lte,
                syntax::BinaryOp::Gt => HirBinaryOp::Gt,
                syntax::BinaryOp::Gte => HirBinaryOp::Gte,
                syntax::BinaryOp::Add => HirBinaryOp::Add,
                syntax::BinaryOp::Sub => HirBinaryOp::Sub,
                syntax::BinaryOp::Mul => HirBinaryOp::Mul,
                syntax::BinaryOp::Div => HirBinaryOp::Div,
                syntax::BinaryOp::Mod => HirBinaryOp::Mod,
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::UnaryOp> {
    type Output = Spanned<HirUnaryOp>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            match self.node {
                syntax::UnaryOp::Neg => HirUnaryOp::Neg,
                syntax::UnaryOp::Not => HirUnaryOp::Not,
            },
            self.span,
        )
    }
}
