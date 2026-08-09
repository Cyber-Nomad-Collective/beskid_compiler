use crate::hir::{HirArrayLiteralExpression, HirExpressionNode, HirIndexExpression, HirPrimitiveType};
use crate::syntax::Spanned;
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(crate) fn type_expression(&mut self, expression: &Spanned<HirExpressionNode>) -> Option<TypeId> {
        let type_id = match &expression.node {
            HirExpressionNode::LambdaExpression(lambda) => self.type_lambda_expression_with_expected(lambda, None),
            HirExpressionNode::LiteralExpression(literal) => self.type_id_for_literal(&literal.node.literal),
            HirExpressionNode::PathExpression(path_expr) => {
                self.type_id_for_path(path_expr.node.path.span, &path_expr.node.path)
            }
            HirExpressionNode::StructLiteralExpression(literal) => self.type_struct_literal_expression(literal),
            HirExpressionNode::EnumConstructorExpression(constructor) => {
                self.type_enum_constructor_expression(constructor)
            }
            HirExpressionNode::AssignExpression(assign) => {
                let target = self.type_expression(&assign.node.target);
                let value = self.type_expression(&assign.node.value);
                if let (Some(target), Some(value)) = (target, value) {
                    let target_is_event_member = match &assign.node.target.node {
                        HirExpressionNode::MemberExpression(member) => self.is_event_member_expression(member),
                        HirExpressionNode::PathExpression(path_expr) => self.is_event_path_expression(path_expr),
                        _ => false,
                    };
                    self.require_same_type(assign.span, target, value);
                    match assign.node.op.node {
                        crate::hir::HirAssignOp::Assign => {}
                        crate::hir::HirAssignOp::AddAssign => {
                            if target_is_event_member {
                                return Some(target);
                            }
                            if matches!(self.type_table.get(value), Some(TypeInfo::Function { .. })) {
                                self.errors.push(TypeError::InvalidEventSubscriptionTarget { span: assign.span });
                                return Some(target);
                            }
                            let is_string = matches!(
                                self.type_table.get(target),
                                Some(TypeInfo::Primitive(HirPrimitiveType::String))
                            );
                            if !self.is_numeric(target) && !is_string {
                                self.errors.push(TypeError::UnsupportedExpression { span: assign.span });
                            }
                        }
                        crate::hir::HirAssignOp::SubAssign => {
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
            HirExpressionNode::BinaryExpression(binary) => self.type_binary_expression(binary),
            HirExpressionNode::UnaryExpression(unary) => self.type_unary_expression(unary),
            HirExpressionNode::GroupedExpression(grouped) => self.type_expression(&grouped.node.expr),
            HirExpressionNode::BlockExpression(block_expr) => {
                self.type_block(&block_expr.node.block);
                self.primitive_type_id(HirPrimitiveType::Unit)
            }
            HirExpressionNode::CallExpression(call) => {
                let type_id = self.type_call_expression(call);
                if let Some(type_id) = type_id {
                    // Inner call span is what codegen uses for method dispatch metadata.
                    self.record_node_type(call.id, type_id);
                }
                type_id
            }
            HirExpressionNode::MemberExpression(member) => self.type_member_expression(member),
            HirExpressionNode::MatchExpression(match_expr) => self.type_match_expression(match_expr),
            HirExpressionNode::TryExpression(try_expr) => self.type_try_expression(try_expr),
            HirExpressionNode::SpawnExpression(spawn_expr) => self.type_spawn_expression(spawn_expr),
            HirExpressionNode::MacroInvocation(_) | HirExpressionNode::MacroMetavariable(_) => {
                self.primitive_type_id(HirPrimitiveType::Unit)
            }
            HirExpressionNode::IndexExpression(index_expr) => self.type_index_expression(index_expr),
            HirExpressionNode::ArrayLiteralExpression(lit) => self.type_array_literal_expression(lit),
            HirExpressionNode::CodeStringExpression(_) => self.primitive_type_id(HirPrimitiveType::String),
            HirExpressionNode::ClifBlockExpression(_) => self.primitive_type_id(HirPrimitiveType::Unit),
        };

        if let Some(type_id) = type_id {
            self.record_node_type(expression.id, type_id);
        }
        type_id
    }

    fn type_index_expression(&mut self, index_expr: &Spanned<HirIndexExpression>) -> Option<TypeId> {
        let target_type = self.type_expression(&index_expr.node.target)?;
        let _index_type = self.type_expression(&index_expr.node.index);

        match self.type_table.get(target_type).cloned() {
            Some(TypeInfo::Array(element_type_id)) => Some(element_type_id),
            Some(TypeInfo::Primitive(HirPrimitiveType::String)) => self.primitive_type_id(HirPrimitiveType::U8),
            _ => {
                self.errors.push(TypeError::UnsupportedExpression { span: index_expr.span });
                None
            }
        }
    }

    fn type_array_literal_expression(&mut self, lit: &Spanned<HirArrayLiteralExpression>) -> Option<TypeId> {
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

    fn type_try_expression(&mut self, try_expr: &Spanned<crate::hir::HirTryExpression>) -> Option<TypeId> {
        let target_type = self.type_expression(&try_expr.node.body)?;
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
