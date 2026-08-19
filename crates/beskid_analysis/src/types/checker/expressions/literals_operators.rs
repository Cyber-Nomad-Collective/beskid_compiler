use crate::syntax::Spanned;
use crate::syntax::{
    BinaryExpression, BinaryOp, Literal, PrimitiveType, UnaryExpression, UnaryOp, integer_literal_primitive_type,
};
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(in crate::types::checker) fn type_binary_expression(
        &mut self,
        binary: &Spanned<BinaryExpression>,
    ) -> Option<TypeId> {
        let left = self.type_expression(&binary.node.left);
        let right = self.type_expression(&binary.node.right);

        if matches!(binary.node.op.node, BinaryOp::Add) {
            let string_add = left.is_some_and(|type_id| self.is_string(type_id))
                || right.is_some_and(|type_id| self.is_string(type_id));
            if string_add {
                return self.primitive_type_id(PrimitiveType::String);
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
            BinaryOp::Or | BinaryOp::And => {
                if self.is_bool(left) {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr => {
                if matches!(
                    self.type_table.get(left),
                    Some(TypeInfo::Primitive(
                        PrimitiveType::I32 | PrimitiveType::I64 | PrimitiveType::U8 | PrimitiveType::Word
                    ))
                ) {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            BinaryOp::IdentityEq | BinaryOp::IdentityNotEq => {
                if self.is_identity_comparable(left) {
                    self.primitive_type_id(PrimitiveType::Bool)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => {
                let ordering =
                    matches!(binary.node.op.node, BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte);
                let comparable = if ordering {
                    self.is_comparable(left)
                } else if self.is_comparable(left) {
                    true
                } else {
                    left == right && self.is_identity_comparable(left)
                };
                if comparable {
                    self.primitive_type_id(PrimitiveType::Bool)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            BinaryOp::Add => {
                if self.is_numeric(left)
                    || matches!(
                        self.type_table.get(left),
                        Some(crate::types::TypeInfo::Primitive(PrimitiveType::String))
                    )
                {
                    Some(left)
                } else {
                    self.errors.push(TypeError::InvalidBinaryOp { span: binary.span });
                    None
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
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
        unary: &Spanned<UnaryExpression>,
    ) -> Option<TypeId> {
        let expr = self.type_expression(&unary.node.expr)?;
        match unary.node.op.node {
            UnaryOp::Neg => {
                if self.is_numeric(expr) {
                    Some(expr)
                } else {
                    self.errors.push(TypeError::InvalidUnaryOp { span: unary.span });
                    None
                }
            }
            UnaryOp::Not => {
                if self.is_bool(expr) {
                    Some(expr)
                } else {
                    self.errors.push(TypeError::InvalidUnaryOp { span: unary.span });
                    None
                }
            }
        }
    }

    pub(in crate::types::checker) fn type_id_for_literal(&mut self, literal: &Spanned<Literal>) -> Option<TypeId> {
        match &literal.node {
            Literal::Integer(text) => self.primitive_type_id(integer_literal_primitive_type(text)),
            Literal::Float(_) => self.primitive_type_id(PrimitiveType::F64),
            Literal::String(_) => self.primitive_type_id(PrimitiveType::String),
            Literal::Char(_) => self.primitive_type_id(PrimitiveType::Char),
            Literal::Bool(_) => self.primitive_type_id(PrimitiveType::Bool),
            Literal::Unit => self.primitive_type_id(PrimitiveType::Unit),
        }
    }
}
