use crate::syntax::{Expression, Pattern, StructLiteralField};
use crate::syntax::Spanned;

use super::super::resolver::Resolver;

impl Resolver {
    pub(super) fn resolve_expression(&mut self, expression: &Spanned<Expression>) {
        match &expression.node {
            Expression::Match(match_expr) => {
                self.resolve_expression(&match_expr.node.scrutinee);
                for arm in &match_expr.node.arms {
                    self.resolve_match_arm(arm);
                }
            }
            Expression::Lambda(lambda_expr) => {
                self.push_scope();
                for parameter in &lambda_expr.node.parameters {
                    if let Some(ty) = &parameter.node.ty {
                        self.resolve_type(ty);
                    }
                    self.insert_local(&parameter.node.name.node.name, parameter.node.name.span);
                }
                self.resolve_expression(&lambda_expr.node.body);
                self.pop_scope();
            }
            Expression::Assign(assign_expr) => {
                self.resolve_expression(&assign_expr.node.target);
                self.resolve_expression(&assign_expr.node.value);
            }
            Expression::Binary(binary_expr) => {
                self.resolve_expression(&binary_expr.node.left);
                self.resolve_expression(&binary_expr.node.right);
            }
            Expression::Unary(unary_expr) => {
                self.resolve_expression(&unary_expr.node.expr);
            }
            Expression::Call(call_expr) => {
                self.resolve_expression(&call_expr.node.callee);
                for arg in &call_expr.node.args {
                    self.resolve_expression(arg);
                }
            }
            Expression::Member(member_expr) => {
                self.resolve_expression(&member_expr.node.target);
            }
            Expression::Literal(_) => {}
            Expression::Path(path_expr) => {
                self.resolve_value_path(&path_expr.node.path);
            }
            Expression::StructLiteral(literal) => {
                self.resolve_type_path(&literal.node.path);
                for field in &literal.node.fields {
                    self.resolve_struct_literal_field(field);
                }
            }
            Expression::EnumConstructor(constructor) => {
                self.resolve_enum_path(&constructor.node.path);
                for arg in &constructor.node.args {
                    self.resolve_expression(arg);
                }
            }
            Expression::Block(block_expr) => {
                self.resolve_block(&block_expr.node.block);
            }
            Expression::Grouped(grouped_expr) => {
                self.resolve_expression(&grouped_expr.node.expr);
            }
            Expression::Try(try_expr) => {
                self.resolve_expression(&try_expr.node.body);
            }
            Expression::Spawn(spawn_expr) => {
                self.resolve_expression(&spawn_expr.node.callee);
            }
            Expression::Index(index_expr) => {
                self.resolve_expression(&index_expr.node.target);
                self.resolve_expression(&index_expr.node.index);
            }
            Expression::ArrayLiteral(lit) => {
                for element in &lit.node.elements {
                    self.resolve_expression(element);
                }
            }
            Expression::MacroInvocation(_) | Expression::MacroMetavariable(_) => {}
            Expression::CodeString(_) => {}
            Expression::ClifBlock(_) => {}
        }
    }

    pub(super) fn resolve_match_arm(&mut self, arm: &Spanned<crate::syntax::MatchArm>) {
        self.push_scope();
        self.resolve_pattern(&arm.node.pattern);
        if let Some(guard) = &arm.node.guard {
            self.resolve_expression(guard);
        }
        self.resolve_expression(&arm.node.value);
        self.pop_scope();
    }

    pub(super) fn resolve_pattern(&mut self, pattern: &Spanned<Pattern>) {
        match &pattern.node {
            Pattern::Wildcard => {}
            Pattern::Identifier(identifier) => {
                self.insert_local(&identifier.node.name, identifier.span);
            }
            Pattern::Literal(_) => {}
            Pattern::Enum(enum_pattern) => {
                self.resolve_enum_path(&enum_pattern.node.path);
                for item in &enum_pattern.node.items {
                    self.resolve_pattern(item);
                }
            }
        }
    }

    pub(super) fn resolve_struct_literal_field(&mut self, field: &Spanned<StructLiteralField>) {
        self.resolve_expression(&field.node.value);
    }
}
