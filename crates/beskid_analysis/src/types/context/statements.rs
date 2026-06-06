use crate::hir::{
    HirBlock, HirElseBranch, HirExpressionNode, HirIfStatement, HirPrimitiveType, HirStatementNode,
};
use crate::syntax::Spanned;

use super::context::{TypeContext, TypeError};

impl<'a> TypeContext<'a> {
    pub(super) fn type_block(&mut self, block: &Spanned<HirBlock>) {
        for statement in &block.node.statements {
            self.type_statement(statement);
        }
    }

    pub(super) fn type_statement(&mut self, statement: &Spanned<HirStatementNode>) {
        match &statement.node {
            HirStatementNode::LetStatement(let_stmt) => match &let_stmt.node.type_annotation {
                Some(ty) => {
                    let expected = self.type_id_for_type(ty);
                    let previous_contextual = self.contextual_expected_type;
                    if let Some(expected_type) = expected {
                        self.contextual_expected_type = Some(expected_type);
                    }
                    let actual = match (expected, &let_stmt.node.value.node) {
                        (Some(expected), HirExpressionNode::LambdaExpression(lambda)) => {
                            self.type_lambda_expression_with_expected(lambda, Some(expected))
                        }
                        (Some(expected), HirExpressionNode::MatchExpression(match_expr)) => self
                            .type_match_expression_with_expected(match_expr, Some(expected)),
                        (Some(_), _) | (None, _) => self.type_expression(&let_stmt.node.value),
                    };
                    self.contextual_expected_type = previous_contextual;
                    if let Some(expected) = expected {
                        if let Some(actual) = actual {
                            self.record_expr_type(let_stmt.node.value.span, actual);
                            self.require_same_type(let_stmt.node.name.span, expected, actual);
                        }
                        self.insert_local_type(let_stmt.node.name.span, expected);
                    } else if let Some(actual) = actual {
                        self.record_expr_type(let_stmt.node.value.span, actual);
                        self.insert_local_type(let_stmt.node.name.span, actual);
                    }
                }
                None => {
                    if let Some(actual) = self.type_expression(&let_stmt.node.value) {
                        if matches!(
                            self.type_table.get(actual),
                            Some(crate::types::TypeInfo::Fiber(_))
                        ) {
                            self.register_fiber_handle_local(
                                let_stmt.node.name.span,
                                let_stmt.node.value.span,
                            );
                        }
                        self.insert_local_type(let_stmt.node.name.span, actual);
                    }
                }
            },
            HirStatementNode::ReturnStatement(return_stmt) => {
                let previous_contextual = self.contextual_expected_type;
                if let Some(expected) = self.current_return_type {
                    self.contextual_expected_type = Some(expected);
                }
                let actual = return_stmt
                    .node
                    .value
                    .as_ref()
                    .and_then(|expr| self.type_expression(expr));
                self.contextual_expected_type = previous_contextual;
                if let Some(expected) = self.current_return_type {
                    match actual {
                        Some(actual) => self.require_same_type(return_stmt.span, expected, actual),
                        None => {
                            if matches!(
                                self.primitive_type_id(HirPrimitiveType::Unit),
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
            HirStatementNode::WhileStatement(while_stmt) => {
                self.require_bool(while_stmt.node.condition.span, &while_stmt.node.condition);
                self.type_block(&while_stmt.node.body);
            }
            HirStatementNode::ForStatement(for_stmt) => {
                if let Some(type_id) = self.resolve_iterable_item_type(&for_stmt.node.iterable) {
                    self.insert_local_type(for_stmt.node.iterator.span, type_id);
                }
                self.type_block(&for_stmt.node.body);
            }
            HirStatementNode::IfStatement(if_stmt) => {
                self.type_if_statement(if_stmt);
            }
            HirStatementNode::ExpressionStatement(expr_stmt) => {
                self.type_expression(&expr_stmt.node.expression);
            }
            HirStatementNode::BreakStatement(_) | HirStatementNode::ContinueStatement(_) => {}
            HirStatementNode::WithStatement(_) | HirStatementNode::LaunchStatement(_) => {}
        }
    }

    pub(super) fn type_if_statement(&mut self, if_stmt: &Spanned<HirIfStatement>) {
        self.require_bool(if_stmt.node.condition.span, &if_stmt.node.condition);
        self.type_block(&if_stmt.node.then_block);
        if let Some(else_branch) = &if_stmt.node.else_branch {
            match &else_branch.node {
                HirElseBranch::Block(block) => self.type_block(block),
                HirElseBranch::If(nested) => self.type_if_statement(nested),
            }
        }
    }
}
