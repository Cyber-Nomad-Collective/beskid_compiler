use crate::hir::{
    HirBinaryExpression, HirBinaryOp, HirLiteral, HirPrimitiveType, HirUnaryExpression, HirUnaryOp,
    integer_literal_primitive_type,
};
use crate::syntax::Spanned;
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(in crate::types::checker) fn type_binary_expression(
        &mut self,
        binary: &Spanned<HirBinaryExpression>,
    ) -> Option<TypeId> {
        let left = self.type_expression(&binary.node.left);
        let right = self.type_expression(&binary.node.right);

        if matches!(binary.node.op.node, HirBinaryOp::Add) {
            let string_add = left.is_some_and(|type_id| self.is_string(type_id))
                || right.is_some_and(|type_id| self.is_string(type_id));
            if string_add {
                return self.primitive_type_id(HirPrimitiveType::String);
            }
        }

        let (left, right) = match (left, right) {
            (Some(left), Some(right)) => self.promote_binary_numeric_operands(left, right),
            _ => return None,
        };
        if left != right {
            self.errors.push(TypeError::TypeMismatch { span: binary.span, expected: left, actual: right });
            return None;
        }
        match binary.node.op.node {
            HirBinaryOp::Or | HirBinaryOp::And => {
                if self.is_bool(left) {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            HirBinaryOp::BitAnd | HirBinaryOp::BitOr | HirBinaryOp::Shl | HirBinaryOp::Shr => {
                if matches!(
                    self.type_table.get(left),
                    Some(TypeInfo::Primitive(
                        HirPrimitiveType::I32 | HirPrimitiveType::I64 | HirPrimitiveType::U8 | HirPrimitiveType::Word
                    ))
                ) {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            HirBinaryOp::IdentityEq | HirBinaryOp::IdentityNotEq => {
                if self.is_identity_comparable(left) {
                    self.primitive_type_id(HirPrimitiveType::Bool)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            HirBinaryOp::Eq
            | HirBinaryOp::NotEq
            | HirBinaryOp::Lt
            | HirBinaryOp::Lte
            | HirBinaryOp::Gt
            | HirBinaryOp::Gte => {
                let ordering = matches!(
                    binary.node.op.node,
                    HirBinaryOp::Lt | HirBinaryOp::Lte | HirBinaryOp::Gt | HirBinaryOp::Gte
                );
                let comparable = if ordering {
                    self.is_comparable(left)
                } else if self.is_comparable(left) {
                    true
                } else {
                    left == right && self.is_identity_comparable(left)
                };
                if comparable {
                    self.primitive_type_id(HirPrimitiveType::Bool)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            HirBinaryOp::Add => {
                if self.is_numeric(left)
                    || matches!(
                        self.type_table.get(left),
                        Some(crate::types::TypeInfo::Primitive(HirPrimitiveType::String))
                    )
                {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            HirBinaryOp::Sub | HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod => {
                if self.is_numeric(left) {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
        }
    }

    pub(in crate::types::checker) fn type_unary_expression(
        &mut self,
        unary: &Spanned<HirUnaryExpression>,
    ) -> Option<TypeId> {
        let expr = self.type_expression(&unary.node.expr)?;
        match unary.node.op.node {
            HirUnaryOp::Neg => {
                if self.is_numeric(expr) {
                    Some(expr)
                } else {
                    self.errors.push(TypeError::InvalidUnaryOp { span: unary.span });
                    None
                }
            }
            HirUnaryOp::Not => {
                if self.is_bool(expr) {
                    Some(expr)
                } else {
                    self.errors.push(TypeError::InvalidUnaryOp { span: unary.span });
                    None
                }
            }
        }
    }

    pub(in crate::types::checker) fn type_id_for_literal(&mut self, literal: &Spanned<HirLiteral>) -> Option<TypeId> {
        match &literal.node {
            HirLiteral::Integer(text) => self.primitive_type_id(integer_literal_primitive_type(text)),
            HirLiteral::Float(_) => self.primitive_type_id(HirPrimitiveType::F64),
            HirLiteral::String(_) => self.primitive_type_id(HirPrimitiveType::String),
            HirLiteral::Char(_) => self.primitive_type_id(HirPrimitiveType::Char),
            HirLiteral::Bool(_) => self.primitive_type_id(HirPrimitiveType::Bool),
        }
    }
}
