use crate::builtins::builtin_for_path;
use crate::syntax::{Expression, LambdaExpression, SpawnExpression, Statement};
use crate::resolve::ResolvedValue;
use crate::syntax::Spanned;
use crate::types::{TypeId, TypeInfo};

use super::TypeChecker;
use crate::types::result::TypeError;

impl<'a> TypeChecker<'a> {
    pub(super) fn type_spawn_expression(&mut self, spawn: &Spanned<SpawnExpression>) -> Option<TypeId> {
        let parent_scope = self.fiber_scope_stack.last().copied().unwrap_or(0);
        let child_scope = self.alloc_fiber_scope(parent_scope);

        let return_type = if let Expression::Lambda(lambda) = &spawn.node.callee.node {
            self.fiber_scope_stack.push(child_scope);
            let typed = self.type_lambda_expression_with_expected(lambda, None);
            self.fiber_scope_stack.pop();
            self.check_spawn_lambda_captures(&spawn.node.callee, spawn.span);
            typed.and_then(|fn_type| self.function_return_type(fn_type))
        } else if let Expression::Call(call) = &spawn.node.callee.node {
            self.spawn_return_type_for_entry(&call.node.callee, spawn.span)
        } else {
            self.spawn_return_type_for_entry(&spawn.node.callee, spawn.span)
        };

        let Some(return_type) = return_type else {
            self.errors.push(TypeError::SpawnTargetNotFiberCompatible { span: spawn.span });
            return None;
        };

        let fiber_type = self
            .fiber_handle_type_for_payload(return_type)
            .unwrap_or_else(|| self.type_table.intern(TypeInfo::Fiber(return_type)));
        self.fiber_handle_scopes.insert(spawn.id, child_scope);
        Some(fiber_type)
    }

    pub(super) fn check_fiber_join_call(
        &mut self,
        join_span: crate::syntax::SpanInfo,
        handle_expr: &Spanned<Expression>,
    ) {
        let Some(handle_scope) = self.fiber_scope_for_expression(handle_expr) else {
            return;
        };
        let current_scope = self.fiber_scope_stack.last().copied().unwrap_or(0);
        if self.fiber_scope_is_strict_ancestor(handle_scope, current_scope) {
            self.errors.push(TypeError::JoinWouldDeadlock { span: join_span });
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

    fn fiber_scope_for_expression(&self, expression: &Spanned<Expression>) -> Option<usize> {
        if let Some(scope) = self.fiber_handle_scopes.get(&expression.id) {
            return Some(*scope);
        }
        match &expression.node {
            Expression::Path(path) => self
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

    fn spawn_return_type_for_entry(
        &mut self,
        entry: &Spanned<Expression>,
        spawn_span: crate::syntax::SpanInfo,
    ) -> Option<TypeId> {
        if let Expression::Path(path) = &entry.node
            && let Some(ResolvedValue::Item(item_id)) = self.resolved_value_at(path.node.path.span)
        {
            return self.function_signatures.get(&item_id).map(|signature| signature.return_type);
        }

        let callee_type = self.type_expression(entry)?;
        self.check_spawn_callee(callee_type, spawn_span);
        self.function_return_type(callee_type)
    }

    fn check_spawn_callee(&mut self, callee_type: TypeId, span: crate::syntax::SpanInfo) {
        if !matches!(self.type_table.get(callee_type), Some(TypeInfo::Function { .. })) {
            self.errors.push(TypeError::SpawnTargetNotFiberCompatible { span });
        }
    }

    fn check_spawn_lambda_captures(&mut self, callee: &Spanned<Expression>, span: crate::syntax::SpanInfo) {
        let Expression::Lambda(lambda) = &callee.node else {
            return;
        };
        if self.lambda_references_outer_stack(lambda) {
            self.errors.push(TypeError::StackReferenceEscapesSpawn { span });
        }
    }

    fn lambda_references_outer_stack(&self, lambda: &Spanned<LambdaExpression>) -> bool {
        let param_locals: std::collections::HashSet<_> =
            lambda.node.parameters.iter().filter_map(|p| self.local_id_for_span(p.node.name.span)).collect();
        self.expression_references_outer_local(lambda.node.body.as_ref(), &param_locals)
    }

    fn expression_references_outer_local(
        &self,
        expression: &Spanned<Expression>,
        param_locals: &std::collections::HashSet<crate::resolve::LocalId>,
    ) -> bool {
        match &expression.node {
            Expression::Path(path) => self
                .resolution
                .tables
                .resolved_values
                .get(&path.node.path.span)
                .and_then(|resolved| match resolved {
                    crate::resolve::ResolvedValue::Local(local_id) => (!param_locals.contains(local_id)).then_some(()),
                    _ => None,
                })
                .is_some(),
            Expression::Lambda(inner) => {
                self.expression_references_outer_local(inner.node.body.as_ref(), param_locals)
            }
            Expression::Unary(unary) => {
                self.expression_references_outer_local(unary.node.expr.as_ref(), param_locals)
            }
            Expression::Grouped(grouped) => {
                self.expression_references_outer_local(grouped.node.expr.as_ref(), param_locals)
            }
            Expression::Call(call) => {
                self.expression_references_outer_local(&call.node.callee, param_locals)
                    || call.node.args.iter().any(|arg| self.expression_references_outer_local(arg, param_locals))
            }
            Expression::Binary(binary) => {
                self.expression_references_outer_local(&binary.node.left, param_locals)
                    || self.expression_references_outer_local(&binary.node.right, param_locals)
            }
            Expression::Assign(assign) => {
                self.expression_references_outer_local(&assign.node.target, param_locals)
                    || self.expression_references_outer_local(&assign.node.value, param_locals)
            }
            Expression::Member(member) => {
                self.expression_references_outer_local(&member.node.target, param_locals)
            }
            Expression::Block(block) => {
                block.node.block.node.statements.iter().any(|stmt| match &stmt.node {
                    Statement::Expression(expr_stmt) => {
                        self.expression_references_outer_local(&expr_stmt.node.expression, param_locals)
                    }
                    Statement::Return(ret) => ret
                        .node
                        .value
                        .as_ref()
                        .is_some_and(|value| self.expression_references_outer_local(value, param_locals)),
                    Statement::Let(let_stmt) => {
                        self.expression_references_outer_local(&let_stmt.node.value, param_locals)
                    }
                    _ => false,
                })
            }
            _ => false,
        }
    }

    pub(super) fn register_fiber_handle_local(
        &mut self,
        local_span: crate::syntax::SpanInfo,
        handle_id: crate::syntax::AstNodeId,
    ) {
        let Some(local_id) = self.local_id_for_span(local_span) else {
            return;
        };
        if let Some(scope) = self.fiber_handle_scopes.get(&handle_id).copied() {
            self.fiber_handle_locals.insert(local_id, scope);
        }
    }

    pub(super) fn is_fiber_join_path(path: &[String]) -> bool {
        builtin_for_path(path).map(|(_, spec)| spec.runtime_symbol == "fiber_join_status").unwrap_or(false)
    }
}
