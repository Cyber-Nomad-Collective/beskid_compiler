use crate::syntax::{Block, ElseBranch, Expression, IfStatement, PrimitiveType, Statement};
use crate::syntax::Spanned;

use super::TypeChecker;
use crate::types::result::TypeError;

impl<'a> TypeChecker<'a> {
    pub(super) fn type_block_inner(&mut self, block: &Spanned<Block>) {
        for statement in &block.node.statements {
            self.type_statement(statement);
        }
    }

    pub(super) fn type_statement(&mut self, statement: &Spanned<Statement>) {
        match &statement.node {
            Statement::Let(let_stmt) => match &let_stmt.node.type_annotation {
                Some(ty) => {
                    let expected = self.type_id_for_type(ty);
                    let previous_contextual = self.contextual_expected_type;
                    if let Some(expected_type) = expected {
                        self.contextual_expected_type = Some(expected_type);
                    }
                    let actual = match (expected, &let_stmt.node.value.node) {
                        (Some(expected), Expression::Lambda(lambda)) => {
                            self.type_lambda_expression_with_expected(lambda, Some(expected))
                        }
                        (Some(expected), Expression::Match(match_expr)) => {
                            self.type_match_expression_with_expected(match_expr, Some(expected))
                        }
                        (Some(_), _) | (None, _) => {
                            self.infer_local_type_from_expression(let_stmt.node.name.span, &let_stmt.node.value)
                        }
                    };
                    self.contextual_expected_type = previous_contextual;
                    if let Some(expected) = expected {
                        if let Some(actual) = actual {
                            self.record_node_type(let_stmt.node.value.id, actual);
                            self.require_same_type(let_stmt.node.name.span, expected, actual);
                        }
                        self.insert_local_type(let_stmt.node.name.span, expected);
                    } else if let Some(actual) = actual {
                        self.record_node_type(let_stmt.node.value.id, actual);
                        self.insert_local_type(let_stmt.node.name.span, actual);
                    }
                }
                None => {
                    if let Some(actual) =
                        self.infer_local_type_from_expression(let_stmt.node.name.span, &let_stmt.node.value)
                    {
                        if matches!(self.type_table.get(actual), Some(crate::types::TypeInfo::Fiber(_))) {
                            self.register_fiber_handle_local(let_stmt.node.name.span, let_stmt.node.value.id);
                        }
                        self.insert_local_type(let_stmt.node.name.span, actual);
                    }
                }
            },
            Statement::Return(return_stmt) => {
                let previous_contextual = self.contextual_expected_type;
                if let Some(expected) = self.current_return_type {
                    self.contextual_expected_type = Some(expected);
                }
                let actual = return_stmt.node.value.as_ref().and_then(|expr| self.type_expression(expr));
                self.contextual_expected_type = previous_contextual;
                if let Some(expected) = self.current_return_type {
                    match actual {
                        Some(actual) => self.require_same_type(return_stmt.span, expected, actual),
                        None => {
                            if matches!(
                                self.primitive_type_id(PrimitiveType::Unit),
                                Some(unit_id) if expected != unit_id
                            ) {
                                self.errors.push(TypeError::ReturnTypeMismatch {
                                    span: return_stmt.span,
                                    expected,
                                    actual: None,
                                });
                            }
                        }
                    }
                }
            }
            Statement::While(while_stmt) => {
                self.require_bool(while_stmt.node.condition.span, &while_stmt.node.condition);
                self.type_block(&while_stmt.node.body);
            }
            Statement::For(for_stmt) => {
                if let Some(type_id) = self.resolve_iterable_item_type(&for_stmt.node.iterable) {
                    self.insert_local_type(for_stmt.node.iterator.span, type_id);
                }
                self.type_block(&for_stmt.node.body);
            }
            Statement::If(if_stmt) => {
                self.type_if_statement(if_stmt);
            }
            Statement::Expression(expr_stmt) => {
                self.type_expression(&expr_stmt.node.expression);
            }
            Statement::Break(_) | Statement::Continue(_) => {}
            Statement::With(_) | Statement::Launch(_) => {}
        }
    }

    pub(super) fn type_if_statement(&mut self, if_stmt: &Spanned<IfStatement>) {
        self.require_bool(if_stmt.node.condition.span, &if_stmt.node.condition);
        self.type_block(&if_stmt.node.then_block);
        if let Some(else_branch) = &if_stmt.node.else_branch {
            match &else_branch.node {
                ElseBranch::Block(block) => self.type_block(block),
                ElseBranch::If(nested) => self.type_if_statement(nested),
            }
        }
    }
}
