use crate::syntax::Spanned;
use crate::syntax::{ArrayLiteralExpression, Expression, IndexExpression, PrimitiveType};
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(crate) fn type_expression(&mut self, expression: &Spanned<Expression>) -> Option<TypeId> {
        let type_id = match &expression.node {
            Expression::Lambda(lambda) => self.type_lambda_expression_with_expected(lambda, None),
            Expression::Literal(literal) => self.type_id_for_literal(&literal.node.literal),
            Expression::Path(path_expr) => self.type_id_for_path(path_expr.node.path.span, &path_expr.node.path),
            Expression::StructLiteral(literal) => self.type_struct_literal_expression(literal),
            Expression::EnumConstructor(constructor) => self.type_enum_constructor_expression(constructor),
            Expression::Assign(assign) => {
                let target = self.type_expression(&assign.node.target);
                let value = self.type_expression(&assign.node.value);
                if let (Some(target), Some(value)) = (target, value) {
                    let target_is_event_member = match &assign.node.target.node {
                        Expression::Member(member) => self.is_event_member_expression(member),
                        Expression::Path(path_expr) => self.is_event_path_expression(path_expr),
                        _ => false,
                    };
                    self.require_same_type(assign.span, target, value);
                    match assign.node.op.node {
                        crate::syntax::AssignOp::Assign => {}
                        crate::syntax::AssignOp::AddAssign => {
                            if target_is_event_member {
                                return Some(target);
                            }
                            if matches!(self.type_table.get(value), Some(TypeInfo::Function { .. })) {
                                self.errors.push(TypeError::InvalidEventSubscriptionTarget { span: assign.span });
                                return Some(target);
                            }
                            let is_string =
                                matches!(self.type_table.get(target), Some(TypeInfo::Primitive(PrimitiveType::String)));
                            if !self.is_numeric(target) && !is_string {
                                self.errors.push(TypeError::UnsupportedExpression { span: assign.span });
                            }
                        }
                        crate::syntax::AssignOp::SubAssign => {
                            if target_is_event_member {
                                return Some(target);
                            }
                            if matches!(self.type_table.get(value), Some(TypeInfo::Function { .. })) {
                                self.errors.push(TypeError::InvalidEventSubscriptionTarget { span: assign.span });
                                return Some(target);
                            }
                            if !self.is_numeric(target) {
                                self.errors.push(TypeError::UnsupportedExpression { span: assign.span });
                            }
                        }
                    }
                    Some(target)
                } else {
                    None
                }
            }
            Expression::Binary(binary) => self.type_binary_expression(binary),
            Expression::Unary(unary) => self.type_unary_expression(unary),
            Expression::Grouped(grouped) => self.type_expression(&grouped.node.expr),
            Expression::Block(block_expr) => {
                self.type_block(&block_expr.node.block);
                self.primitive_type_id(PrimitiveType::Unit)
            }
            Expression::Call(call) => {
                let type_id = self.type_call_expression(call);
                if let Some(type_id) = type_id {
                    // Inner call span is what codegen uses for method dispatch metadata.
                    self.record_node_type(call.id, type_id);
                }
                type_id
            }
            Expression::Member(member) => self.type_member_expression(member),
            Expression::Match(match_expr) => self.type_match_expression(match_expr),
            Expression::Try(try_expr) => self.type_try_expression(try_expr),
            Expression::Spawn(spawn_expr) => self.type_spawn_expression(spawn_expr),
            Expression::MacroInvocation(_) | Expression::MacroMetavariable(_) => {
                self.primitive_type_id(PrimitiveType::Unit)
            }
            Expression::Index(index_expr) => self.type_index_expression(index_expr),
            Expression::ArrayLiteral(lit) => self.type_array_literal_expression(lit),
            Expression::CodeString(_) => self.primitive_type_id(PrimitiveType::String),
            Expression::ClifBlock(_) => self.primitive_type_id(PrimitiveType::Unit),
        };

        if let Some(type_id) = type_id {
            self.record_node_type(expression.id, type_id);
        }
        type_id
    }

    fn type_index_expression(&mut self, index_expr: &Spanned<IndexExpression>) -> Option<TypeId> {
        let target_type = self.type_expression(&index_expr.node.target)?;
        let _index_type = self.type_expression(&index_expr.node.index);

        match self.type_table.get(target_type).cloned() {
            Some(TypeInfo::Array(element_type_id)) => Some(element_type_id),
            Some(TypeInfo::Primitive(PrimitiveType::String)) => self.primitive_type_id(PrimitiveType::U8),
            _ => {
                self.errors.push(TypeError::UnsupportedExpression { span: index_expr.span });
                None
            }
        }
    }

    fn type_array_literal_expression(&mut self, lit: &Spanned<ArrayLiteralExpression>) -> Option<TypeId> {
        if lit.node.elements.is_empty() {
            return None;
        }

        let first_type = self.type_expression(&lit.node.elements[0])?;

        for elem in &lit.node.elements[1..] {
            let elem_type = self.type_expression(elem);
            if let Some(elem_type) = elem_type
                && elem_type != first_type
            {
                self.errors.push(TypeError::UnsupportedExpression { span: lit.span });
                return None;
            }
        }

        Some(self.type_table.intern(TypeInfo::Array(first_type)))
    }

    fn type_try_expression(&mut self, try_expr: &Spanned<crate::syntax::TryExpression>) -> Option<TypeId> {
        let target_type = self.type_expression(&try_expr.node.expr)?;
        let Some(result_item_id) = self.named_item_id(target_type) else {
            self.errors.push(TypeError::InvalidTryTarget { span: try_expr.span });
            return None;
        };

        let ok_fields = self
            .enum_variants_ordered
            .get(&result_item_id)
            .and_then(|variants| variants.iter().find(|(name, _)| name == "Ok").map(|(_, fields)| fields.clone()));
        let Some(fields) = ok_fields else {
            self.errors.push(TypeError::InvalidTryTarget { span: try_expr.span });
            return None;
        };

        match self.type_table.get(target_type).cloned() {
            Some(TypeInfo::Applied { args, .. }) => {
                if let Some(payload_type) = args.first().copied() {
                    Some(payload_type)
                } else if fields.len() == 1 {
                    Some(fields[0])
                } else {
                    self.errors.push(TypeError::InvalidTryTarget { span: try_expr.span });
                    None
                }
            }
            Some(TypeInfo::Named(_)) => {
                if fields.len() == 1 {
                    Some(fields[0])
                } else {
                    self.errors.push(TypeError::InvalidTryTarget { span: try_expr.span });
                    None
                }
            }
            _ => {
                self.errors.push(TypeError::InvalidTryTarget { span: try_expr.span });
                None
            }
        }
    }
}
