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
        // An unsuffixed integer literal used directly as one operand selects the exact primitive
        // integer representation its sibling already proves. This is representation selection,
        // not widening: a magnitude that does not fit that type is rejected.
        use crate::types::inference::generic::is_bare_integer_literal;
        let left_literal = is_bare_integer_literal(&binary.node.left.node);
        let right_literal = is_bare_integer_literal(&binary.node.right.node);
        let (left, right) = if right_literal && !left_literal {
            let left = self.type_expression(&binary.node.left);
            let right = self.type_literal_operand(&binary.node.right, left);
            (left, right)
        } else if left_literal && !right_literal {
            let right = self.type_expression(&binary.node.right);
            let left = self.type_literal_operand(&binary.node.left, right);
            (left, right)
        } else {
            (self.type_expression(&binary.node.left), self.type_expression(&binary.node.right))
        };

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
                        PrimitiveType::I8
                            | PrimitiveType::I16
                            | PrimitiveType::U16
                            | PrimitiveType::U64
                            | PrimitiveType::I32
                            | PrimitiveType::I64
                            | PrimitiveType::U32
                            | PrimitiveType::U8
                            | PrimitiveType::Word
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
        // The negative minimum has a positive magnitude one above the signed maximum.
        // Validate the complete unary literal before its positive operand is checked alone.
        if unary.node.op.node == UnaryOp::Neg
            && let crate::syntax::Expression::Literal(value) = &unary.node.expr.node
            && let Literal::Integer(text) = &value.node.literal.node
        {
            let declared = integer_literal_primitive_type(text);
            let primitive = if crate::syntax::integer_literal_magnitude(text) == text {
                self.contextual_expected_type
                    .and_then(|id| match self.type_table.get(id) {
                        Some(TypeInfo::Primitive(p))
                            if matches!(
                                p,
                                PrimitiveType::I8
                                    | PrimitiveType::I16
                                    | PrimitiveType::I32
                                    | PrimitiveType::I64
                                    | PrimitiveType::U8
                                    | PrimitiveType::U16
                                    | PrimitiveType::U32
                                    | PrimitiveType::U64
                                    | PrimitiveType::Word
                            ) =>
                        {
                            Some(*p)
                        }
                        _ => None,
                    })
                    .unwrap_or(declared)
            } else {
                declared
            };
            let negative = format!("-{text}");
            if crate::syntax::integer_literal_fits_primitive(&negative, primitive) {
                return self.primitive_type_id(primitive);
            }
            self.errors.push(TypeError::NumericLiteralOutOfRange { span: unary.span, primitive });
            return None;
        }
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

    /// Type a bare integer literal operand in the representation of its sibling operand when
    /// that sibling is a primitive integer; otherwise keep the surrounding context.
    fn type_literal_operand(
        &mut self,
        operand: &Spanned<crate::syntax::Expression>,
        sibling: Option<TypeId>,
    ) -> Option<TypeId> {
        let Some(sibling) =
            sibling.filter(|id| crate::types::inference::generic::is_integer_primitive(&self.type_table, *id))
        else {
            return self.type_expression(operand);
        };
        let previous = self.contextual_expected_type;
        self.contextual_expected_type = Some(sibling);
        let result = self.type_expression(operand);
        self.contextual_expected_type = previous;
        result
    }

    pub(in crate::types::checker) fn type_id_for_literal(&mut self, literal: &Spanned<Literal>) -> Option<TypeId> {
        match &literal.node {
            Literal::Integer(text) => {
                let inferred = integer_literal_primitive_type(text);
                let primitive = if crate::syntax::integer_literal_magnitude(text) == text {
                    self.contextual_expected_type
                        .and_then(|id| match self.type_table.get(id) {
                            Some(TypeInfo::Primitive(p))
                                if matches!(
                                    p,
                                    PrimitiveType::I8
                                        | PrimitiveType::I16
                                        | PrimitiveType::I32
                                        | PrimitiveType::I64
                                        | PrimitiveType::U8
                                        | PrimitiveType::U16
                                        | PrimitiveType::U32
                                        | PrimitiveType::U64
                                        | PrimitiveType::Word
                                ) =>
                            {
                                Some(*p)
                            }
                            _ => None,
                        })
                        .unwrap_or(inferred)
                } else {
                    inferred
                };
                if !crate::syntax::integer_literal_fits_primitive(text, primitive) {
                    self.errors.push(TypeError::NumericLiteralOutOfRange { span: literal.span, primitive });
                    return None;
                }
                self.primitive_type_id(primitive)
            }
            Literal::Float(text) => {
                let primitive = if !text.ends_with("_f32") && !text.ends_with("_f64") {
                    self.contextual_expected_type
                        .and_then(|id| match self.type_table.get(id) {
                            Some(TypeInfo::Primitive(p @ (PrimitiveType::F32 | PrimitiveType::F64))) => Some(*p),
                            _ => None,
                        })
                        .unwrap_or(PrimitiveType::F64)
                } else {
                    crate::syntax::float_literal_primitive_type(text)
                };
                let magnitude = crate::syntax::float_literal_magnitude(text);
                let valid = match primitive {
                    PrimitiveType::F32 => magnitude.parse::<f32>().is_ok_and(|v| v.is_finite()),
                    PrimitiveType::F64 => magnitude.parse::<f64>().is_ok_and(|v| v.is_finite()),
                    _ => false,
                };
                if !valid {
                    self.errors.push(TypeError::NumericLiteralOutOfRange { span: literal.span, primitive });
                    return None;
                }
                self.primitive_type_id(primitive)
            }
            Literal::String(_) => self.primitive_type_id(PrimitiveType::String),
            Literal::Char(_) => self.primitive_type_id(PrimitiveType::Char),
            Literal::Bool(_) => self.primitive_type_id(PrimitiveType::Bool),
            Literal::Unit => self.primitive_type_id(PrimitiveType::Unit),
        }
    }
}
