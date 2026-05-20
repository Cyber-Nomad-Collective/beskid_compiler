use crate::builtins::builtin_for_path;
use crate::hir::{HirExpressionNode, HirLambdaExpression, HirSpawnExpression, HirStatementNode};
use crate::syntax::Spanned;
use crate::types::{TypeId, TypeInfo};

use super::context::{TypeContext, TypeError};

impl<'a> TypeContext<'a> {
    pub(super) fn type_spawn_expression(
        &mut self,
        spawn: &Spanned<HirSpawnExpression>,
    ) -> Option<TypeId> {
        let parent_scope = self.fiber_scope_stack.last().copied().unwrap_or(0);
        let child_scope = self.alloc_fiber_scope(parent_scope);

        let return_type =
            if let HirExpressionNode::LambdaExpression(lambda) = &spawn.node.callee.node {
                self.fiber_scope_stack.push(child_scope);
                let typed = self.type_lambda_expression_with_expected(lambda, None);
                self.fiber_scope_stack.pop();
                self.check_spawn_lambda_captures(&spawn.node.callee, spawn.span);
                typed.and_then(|fn_type| self.function_return_type(fn_type))
            } else {
                let callee_type = self.type_expression(&spawn.node.callee)?;
                self.check_spawn_callee(callee_type, spawn.span);
                self.function_return_type(callee_type)
            };

        let Some(return_type) = return_type else {
            self.errors
                .push(TypeError::SpawnTargetNotFiberCompatible { span: spawn.span });
            return None;
        };

        let fiber_type = self.type_table.intern(TypeInfo::Fiber(return_type));
        self.fiber_handle_scopes.insert(spawn.span, child_scope);
        Some(fiber_type)
    }

    pub(super) fn check_fiber_join_call(
        &mut self,
        join_span: crate::syntax::SpanInfo,
        handle_expr: &Spanned<HirExpressionNode>,
    ) {
        let Some(handle_scope) = self.fiber_scope_for_expression(handle_expr) else {
            return;
        };
        let current_scope = self.fiber_scope_stack.last().copied().unwrap_or(0);
        if self.fiber_scope_is_strict_ancestor(handle_scope, current_scope) {
            self.errors
                .push(TypeError::JoinWouldDeadlock { span: join_span });
        }
    }

    fn alloc_fiber_scope(&mut self, parent: usize) -> usize {
        let id = self.next_fiber_scope;
        self.next_fiber_scope += 1;
        self.fiber_scope_parent.insert(id, parent);
        id
    }

    fn fiber_scope_is_strict_ancestor(&self, ancestor: usize, descendant: usize) -> bool {
        let mut current = self.fiber_scope_parent.get(&descendant).copied();
        while let Some(scope) = current {
            if scope == ancestor {
                return true;
            }
            current = self.fiber_scope_parent.get(&scope).copied();
        }
        false
    }

    fn fiber_scope_for_expression(&self, expression: &Spanned<HirExpressionNode>) -> Option<usize> {
        if let Some(scope) = self.fiber_handle_scopes.get(&expression.span) {
            return Some(*scope);
        }
        match &expression.node {
            HirExpressionNode::PathExpression(path) => self
                .local_id_for_span(path.node.path.span)
                .and_then(|local| self.fiber_handle_locals.get(&local).copied()),
            _ => None,
        }
    }

    fn function_return_type(&self, fn_type: TypeId) -> Option<TypeId> {
        match self.type_table.get(fn_type)? {
            TypeInfo::Function { return_type, .. } => Some(*return_type),
            _ => None,
        }
    }

    fn check_spawn_callee(&mut self, callee_type: TypeId, span: crate::syntax::SpanInfo) {
        if !matches!(
            self.type_table.get(callee_type),
            Some(TypeInfo::Function { .. })
        ) {
            self.errors
                .push(TypeError::SpawnTargetNotFiberCompatible { span });
        }
    }

    fn check_spawn_lambda_captures(
        &mut self,
        callee: &Spanned<HirExpressionNode>,
        span: crate::syntax::SpanInfo,
    ) {
        let HirExpressionNode::LambdaExpression(lambda) = &callee.node else {
            return;
        };
        if self.lambda_references_outer_stack(lambda) {
            self.errors
                .push(TypeError::StackReferenceEscapesSpawn { span });
        }
    }

    fn lambda_references_outer_stack(&self, lambda: &Spanned<HirLambdaExpression>) -> bool {
        let param_locals: std::collections::HashSet<_> = lambda
            .node
            .parameters
            .iter()
            .filter_map(|p| self.local_id_for_span(p.node.name.span))
            .collect();
        self.expression_references_outer_local(lambda.node.body.as_ref(), &param_locals)
    }

    fn expression_references_outer_local(
        &self,
        expression: &Spanned<HirExpressionNode>,
        param_locals: &std::collections::HashSet<crate::resolve::LocalId>,
    ) -> bool {
        match &expression.node {
            HirExpressionNode::PathExpression(path) => self
                .resolution
                .tables
                .resolved_values
                .get(&path.node.path.span)
                .and_then(|resolved| match resolved {
                    crate::resolve::ResolvedValue::Local(local_id) => {
                        (!param_locals.contains(local_id)).then_some(())
                    }
                    _ => None,
                })
                .is_some(),
            HirExpressionNode::LambdaExpression(inner) => {
                self.expression_references_outer_local(inner.node.body.as_ref(), param_locals)
            }
            HirExpressionNode::UnaryExpression(unary) => {
                self.expression_references_outer_local(unary.node.expr.as_ref(), param_locals)
            }
            HirExpressionNode::GroupedExpression(grouped) => {
                self.expression_references_outer_local(grouped.node.expr.as_ref(), param_locals)
            }
            HirExpressionNode::CallExpression(call) => {
                self.expression_references_outer_local(&call.node.callee, param_locals)
                    || call
                        .node
                        .args
                        .iter()
                        .any(|arg| self.expression_references_outer_local(arg, param_locals))
            }
            HirExpressionNode::BinaryExpression(binary) => {
                self.expression_references_outer_local(&binary.node.left, param_locals)
                    || self.expression_references_outer_local(&binary.node.right, param_locals)
            }
            HirExpressionNode::AssignExpression(assign) => {
                self.expression_references_outer_local(&assign.node.target, param_locals)
                    || self.expression_references_outer_local(&assign.node.value, param_locals)
            }
            HirExpressionNode::MemberExpression(member) => {
                self.expression_references_outer_local(&member.node.target, param_locals)
            }
            HirExpressionNode::BlockExpression(block) => block
                .node
                .block
                .node
                .statements
                .iter()
                .any(|stmt| match &stmt.node {
                    HirStatementNode::ExpressionStatement(expr_stmt) => self
                        .expression_references_outer_local(
                            &expr_stmt.node.expression,
                            param_locals,
                        ),
                    HirStatementNode::ReturnStatement(ret) => {
                        ret.node.value.as_ref().is_some_and(|value| {
                            self.expression_references_outer_local(value, param_locals)
                        })
                    }
                    HirStatementNode::LetStatement(let_stmt) => {
                        self.expression_references_outer_local(&let_stmt.node.value, param_locals)
                    }
                    _ => false,
                }),
            _ => false,
        }
    }

    pub(super) fn register_fiber_handle_local(
        &mut self,
        local_span: crate::syntax::SpanInfo,
        handle_span: crate::syntax::SpanInfo,
    ) {
        let Some(local_id) = self.local_id_for_span(local_span) else {
            return;
        };
        if let Some(scope) = self.fiber_handle_scopes.get(&handle_span).copied() {
            self.fiber_handle_locals.insert(local_id, scope);
        }
    }

    pub(super) fn is_fiber_join_path(path: &[String]) -> bool {
        builtin_for_path(path)
            .map(|(_, spec)| spec.runtime_symbol == "fiber_join_status")
            .unwrap_or(false)
    }
}
