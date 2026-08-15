use crate::builtins::{BuiltinType, builtin_specs};
use crate::syntax::{
    CallExpression, Expression, LambdaExpression, Literal, PrimitiveType, integer_literal_magnitude,
};
use crate::resolve::ResolvedValue;
use crate::syntax::Spanned;
use crate::types::path_value::{method_name_from_path_callee, receiver_type_for_path_callee};
use crate::types::result::{CallLoweringKind, MethodReceiverSource, TypeError};
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(in crate::types::checker) fn type_lambda_expression_with_expected(
        &mut self,
        lambda: &Spanned<LambdaExpression>,
        expected_function: Option<TypeId>,
    ) -> Option<TypeId> {
        let expected_signature = expected_function.and_then(|type_id| match self.type_table.get(type_id) {
            Some(TypeInfo::Function { params, return_type }) => Some((params.clone(), *return_type, type_id)),
            _ => None,
        });

        let mut params = Vec::with_capacity(lambda.node.parameters.len());
        let mut missing = false;

        for (index, parameter) in lambda.node.parameters.iter().enumerate() {
            let inferred =
                expected_signature.as_ref().and_then(|(expected_params, _, _)| expected_params.get(index).copied());
            let type_id = if let Some(ty) = &parameter.node.ty {
                let Some(type_id) = self.type_id_for_type(ty) else {
                    missing = true;
                    continue;
                };
                type_id
            } else if let Some(type_id) = inferred {
                type_id
            } else {
                self.errors.push(TypeError::MissingTypeAnnotation {
                    span: parameter.span,
                    name: parameter.node.name.node.name.clone(),
                });
                missing = true;
                continue;
            };
            self.insert_local_type(parameter.node.name.span, type_id);
            params.push(type_id);
        }

        if let Some((expected_params, _, _)) = &expected_signature
            && expected_params.len() != params.len()
        {
            self.errors.push(TypeError::CallArityMismatch {
                span: lambda.span,
                expected: expected_params.len(),
                actual: params.len(),
            });
            return None;
        }

        let return_type = self.type_expression(&lambda.node.body)?;
        if let Some((_, expected_return, _)) = expected_signature {
            self.require_same_type(lambda.node.body.span, expected_return, return_type);
        }
        if missing {
            return None;
        }

        let actual = self.type_table.intern(TypeInfo::Function { params, return_type });
        if let Some((_, _, expected_type_id)) = expected_signature {
            return Some(expected_type_id);
        }
        Some(actual)
    }

    fn type_argument_with_expected(&mut self, arg: &Spanned<Expression>, expected: TypeId) -> Option<TypeId> {
        match &arg.node {
            Expression::Lambda(lambda) => {
                self.type_lambda_expression_with_expected(lambda, Some(expected))
            }
            Expression::Grouped(grouped) => match &grouped.node.expr.node {
                Expression::Lambda(lambda) => {
                    self.type_lambda_expression_with_expected(lambda, Some(expected))
                }
                _ => self.type_expression(arg),
            },
            _ => self.type_expression(arg),
        }
    }

    pub(in crate::types::checker) fn type_call_expression(
        &mut self,
        call: &Spanned<CallExpression>,
    ) -> Option<TypeId> {
        if let Some((receiver_source, receiver_type, receiver_item_id, field_type)) =
            self.resolve_event_call_target(&call.node.callee)
        {
            let TypeInfo::Function { params, return_type } =
                self.type_table.get(field_type).cloned().unwrap_or(TypeInfo::Primitive(PrimitiveType::Unit))
            else {
                self.errors.push(TypeError::UnknownCallTarget { span: call.span });
                return None;
            };

            if call.node.args.len() != params.len() {
                self.errors.push(TypeError::CallArityMismatch {
                    span: call.span,
                    expected: params.len(),
                    actual: call.node.args.len(),
                });
                return Some(return_type);
            }

            for (arg, expected) in call.node.args.iter().zip(params.iter()) {
                if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                    self.require_same_type(arg.span, *expected, actual);
                }
            }

            if self.current_receiver_item_id != Some(receiver_item_id) {
                self.errors.push(TypeError::InvalidEventInvocationScope { span: call.span });
            }

            self.record_call_kind(call.id, CallLoweringKind::EventInvoke { receiver_source, receiver_type });
            return Some(return_type);
        }

        if let Expression::Path(path_expr) = &call.node.callee.node {
            let path: Vec<String> =
                path_expr.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
            if Self::is_fiber_join_path(&path)
                && let Some(handle) = call.node.args.first()
            {
                self.check_fiber_join_call(call.span, handle);
            }
            let segments = &path_expr.node.path.node.segments;
            let source_path = self.current_source_path.as_ref();
            if segments.len() >= 2
                && let Some(method_name) = method_name_from_path_callee(segments)
                && let Some((local_id, receiver_type)) = receiver_type_for_path_callee(
                    self.resolution,
                    &self.path_env(),
                    path_expr.node.path.span,
                    segments,
                    source_path,
                )
            {
                if let Some(method_item_id) = self.method_item_for_receiver(receiver_type, method_name) {
                    let Some(signature) = self.method_dispatch_signature(method_item_id, receiver_type) else {
                        self.errors.push(TypeError::UnknownCallTarget { span: call.node.callee.span });
                        return None;
                    };
                    let param_types = &signature.params;

                    if call.node.args.len() != param_types.len() {
                        self.errors.push(TypeError::CallArityMismatch {
                            span: call.span,
                            expected: param_types.len(),
                            actual: call.node.args.len(),
                        });
                        return Some(signature.return_type);
                    }

                    for (arg, expected) in call.node.args.iter().zip(param_types.iter()) {
                        if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                            self.require_same_type(arg.span, *expected, actual);
                        }
                    }
                    self.record_call_kind(
                        call.id,
                        CallLoweringKind::MethodDispatch {
                            method_item_id,
                            receiver_source: MethodReceiverSource::Local(local_id),
                            receiver_type,
                        },
                    );
                    return Some(signature.return_type);
                }
                if let Some(contract_item_id) = self.named_item_id(receiver_type)
                    && let Some(signature) =
                        self.contract_signatures.get(&(contract_item_id, method_name.to_string())).cloned()
                {
                    if call.node.args.len() != signature.params.len() {
                        self.errors.push(TypeError::CallArityMismatch {
                            span: call.span,
                            expected: signature.params.len(),
                            actual: call.node.args.len(),
                        });
                        return Some(signature.return_type);
                    }

                    for (arg, expected) in call.node.args.iter().zip(signature.params.iter()) {
                        if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                            self.require_same_type(arg.span, *expected, actual);
                        }
                    }
                    self.record_call_kind(
                        call.id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id,
                            receiver_source: MethodReceiverSource::Local(local_id),
                            receiver_type,
                        },
                    );
                    return Some(signature.return_type);
                }
            }

            // Contract-as-namespace call using a dotted PathExpression: `C.getpid(...)`
            let resolved = self.resolved_value_at(path_expr.node.path.span);
            if segments.len() >= 2
                && let Some(ResolvedValue::Item(contract_item_id)) = resolved
                && let Some(method_name) = method_name_from_path_callee(segments)
                && let Some(signature) =
                    self.contract_signatures.get(&(contract_item_id, method_name.to_string())).cloned()
            {
                if call.node.args.len() != signature.params.len() {
                    self.errors.push(TypeError::CallArityMismatch {
                        span: call.span,
                        expected: signature.params.len(),
                        actual: call.node.args.len(),
                    });
                    return Some(signature.return_type);
                }
                for (arg, expected) in call.node.args.iter().zip(signature.params.iter()) {
                    if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                        self.require_same_type(arg.span, *expected, actual);
                    }
                }
                let receiver_type = self
                    .named_types
                    .get(&contract_item_id)
                    .copied()
                    .unwrap_or_else(|| self.type_table.intern(TypeInfo::Named(contract_item_id)));
                self.record_call_kind(
                    call.id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id,
                        receiver_source: MethodReceiverSource::Expression(path_expr.node.path.span),
                        receiver_type,
                    },
                );
                return Some(signature.return_type);
            }
        }

        if let Expression::Member(member) = &call.node.callee.node {
            // Special-case: contract-as-namespace calls like `C.getpid()` where `C` is a contract item.
            if let Expression::Path(path_expr) = &member.node.target.node
                && let Some(ResolvedValue::Item(item_id)) = self.resolved_value_at(path_expr.node.path.span)
            {
                let method_name = member.node.member.node.name.as_str().to_string();
                if let Some(signature) = self.contract_signatures.get(&(item_id, method_name.clone())).cloned() {
                    if call.node.args.len() != signature.params.len() {
                        self.errors.push(TypeError::CallArityMismatch {
                            span: call.span,
                            expected: signature.params.len(),
                            actual: call.node.args.len(),
                        });
                        return Some(signature.return_type);
                    }
                    for (arg, expected) in call.node.args.iter().zip(signature.params.iter()) {
                        if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                            self.require_same_type(arg.span, *expected, actual);
                        }
                    }
                    let receiver_type = self
                        .named_types
                        .get(&item_id)
                        .copied()
                        .unwrap_or_else(|| self.type_table.intern(TypeInfo::Named(item_id)));
                    self.record_call_kind(
                        call.id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id: item_id,
                            receiver_source: MethodReceiverSource::Expression(member.node.target.span),
                            receiver_type,
                        },
                    );
                    return Some(signature.return_type);
                }
            }

            let target_type = self.type_expression(&member.node.target)?;
            let method_name = member.node.member.node.name.as_str();
            if let Some(method_item_id) = self.method_item_for_receiver(target_type, method_name) {
                let Some(signature) = self.method_dispatch_signature(method_item_id, target_type) else {
                    self.errors.push(TypeError::UnknownCallTarget { span: call.node.callee.span });
                    return None;
                };

                if call.node.args.len() != signature.params.len() {
                    self.errors.push(TypeError::CallArityMismatch {
                        span: call.span,
                        expected: signature.params.len(),
                        actual: call.node.args.len(),
                    });
                    return Some(signature.return_type);
                }

                for (arg, expected) in call.node.args.iter().zip(signature.params.iter()) {
                    if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                        self.require_same_type(arg.span, *expected, actual);
                    }
                }
                self.record_call_kind(
                    call.id,
                    CallLoweringKind::MethodDispatch {
                        method_item_id,
                        receiver_source: MethodReceiverSource::Expression(member.node.target.span),
                        receiver_type: target_type,
                    },
                );
                return Some(signature.return_type);
            }
            if let Some(contract_item_id) = self.named_item_id(target_type)
                && let Some(signature) =
                    self.contract_signatures.get(&(contract_item_id, method_name.to_string())).cloned()
            {
                if call.node.args.len() != signature.params.len() {
                    self.errors.push(TypeError::CallArityMismatch {
                        span: call.span,
                        expected: signature.params.len(),
                        actual: call.node.args.len(),
                    });
                    return Some(signature.return_type);
                }

                for (arg, expected) in call.node.args.iter().zip(signature.params.iter()) {
                    if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                        self.require_same_type(arg.span, *expected, actual);
                    }
                }
                self.record_call_kind(
                    call.id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id,
                        receiver_source: MethodReceiverSource::Expression(member.node.target.span),
                        receiver_type: target_type,
                    },
                );
                return Some(signature.return_type);
            }
        }

        let is_item_callee = match &call.node.callee.node {
            Expression::Path(path_expr) => {
                matches!(self.resolved_value_at(path_expr.node.path.span), Some(ResolvedValue::Item(_)))
            }
            _ => false,
        };

        if !is_item_callee
            && let Some(callee_type) = self.type_expression(&call.node.callee)
            && let Some(TypeInfo::Function { params, return_type }) = self.type_table.get(callee_type).cloned()
        {
            if call.node.args.len() != params.len() {
                self.errors.push(TypeError::CallArityMismatch {
                    span: call.span,
                    expected: params.len(),
                    actual: call.node.args.len(),
                });
                return Some(return_type);
            }

            for (arg, expected) in call.node.args.iter().zip(params.iter()) {
                if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                    self.require_same_type(arg.span, *expected, actual);
                }
            }
            self.record_call_kind(call.id, CallLoweringKind::CallableValueCall);
            return Some(return_type);
        }

        let mut generic_args: Option<Vec<TypeId>> = None;
        let mut generic_expected: Option<usize> = None;
        let mut callee_item_id = None;
        let mut builtin_param_kinds: Option<Vec<BuiltinType>> = None;
        let signature = match &call.node.callee.node {
            Expression::Path(path_expr) => {
                let span = path_expr.node.path.span;
                let segments = &path_expr.node.path.node.segments;
                if let Some(last_segment) = segments.last()
                    && !last_segment.node.type_args.is_empty()
                {
                    let mut args = Vec::with_capacity(last_segment.node.type_args.len());
                    for arg in &last_segment.node.type_args {
                        args.push(self.type_id_for_type(arg)?);
                    }
                    generic_args = Some(args);
                } else if generic_args.is_none() {
                    generic_args = self.infer_generic_args_from_qualified_type_path(segments);
                }
                match self.resolved_value_at(span) {
                    Some(ResolvedValue::Item(item_id)) => {
                        callee_item_id = Some(item_id);
                        if let Some(index) = self.resolution.builtin_items.get(&item_id)
                            && let Some(spec) = builtin_specs().get(*index)
                        {
                            builtin_param_kinds = Some(spec.params.to_vec());
                        }
                        if let Some(expected) = self.generic_items.get(&item_id) {
                            generic_expected = Some(expected.len());
                        }
                        self.function_signatures.get(&item_id).cloned()
                    }
                    _ => None,
                }
            }
            _ => None,
        };

        let Some(signature) = signature else {
            self.errors.push(TypeError::UnknownCallTarget { span: call.span });
            return None;
        };

        if let Some(item_id) = callee_item_id {
            self.record_call_kind(call.id, CallLoweringKind::ItemCall { item_id });
        }

        if let Some(expected) = generic_expected {
            match &generic_args {
                Some(args) => {
                    if args.len() != expected {
                        self.errors.push(TypeError::GenericArgumentMismatch {
                            span: call.span,
                            expected,
                            actual: args.len(),
                        });
                        return Some(signature.return_type);
                    }
                }
                None => {
                    if expected != 0 {
                        let arg_types =
                            call.node.args.iter().filter_map(|arg| self.type_expression(arg)).collect::<Vec<_>>();
                        if let Some(item_id) = callee_item_id {
                            self.record_generic_call_constraints(item_id, &arg_types, expected, call.span);
                        }
                        if let Some(inferred) = self.infer_generic_args_from_call(callee_item_id, &call.node.args) {
                            generic_args = Some(inferred);
                        } else {
                            self.errors.push(TypeError::MissingTypeArguments { span: call.span });
                            return Some(signature.return_type);
                        }
                    }
                }
            }
        } else if let Some(args) = &generic_args
            && !args.is_empty()
        {
            self.errors.push(TypeError::GenericArgumentMismatch { span: call.span, expected: 0, actual: args.len() });
            return Some(signature.return_type);
        }

        let substitution = generic_args.clone().unwrap_or_default();

        let mapping =
            callee_item_id.map(|item_id| self.generic_substitution_mapping(item_id, &substitution)).unwrap_or_default();

        let substituted_params = if mapping.is_empty() {
            signature.params.clone()
        } else {
            signature.params.iter().map(|param| self.substitute_type_id(*param, &mapping)).collect()
        };

        let substituted_return = if mapping.is_empty() {
            signature.return_type
        } else {
            self.substitute_type_id(signature.return_type, &mapping)
        };

        let expected_arity = builtin_param_kinds.as_ref().map(std::vec::Vec::len).unwrap_or(substituted_params.len());

        if call.node.args.len() != expected_arity {
            self.errors.push(TypeError::CallArityMismatch {
                span: call.span,
                expected: expected_arity,
                actual: call.node.args.len(),
            });
            return Some(substituted_return);
        }

        if let Some(kinds) = builtin_param_kinds.as_ref() {
            let mut typed_index = 0usize;
            for (arg, kind) in call.node.args.iter().zip(kinds.iter()) {
                let _ = self.type_expression(arg);
                if matches!(kind, BuiltinType::Ptr) {
                    continue;
                }
                if let Some(expected) = substituted_params.get(typed_index) {
                    if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                        self.require_same_type(arg.span, *expected, actual);
                    }
                    typed_index += 1;
                }
            }
        } else {
            for (arg, expected) in call.node.args.iter().zip(substituted_params.iter()) {
                if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                    self.require_same_type(arg.span, *expected, actual);
                }
            }
        }

        let mut return_type = substituted_return;
        if let Some(item_id) = callee_item_id
            && let Some(index) = self.resolution.builtin_items.get(&item_id)
            && let Some(spec) = crate::builtins::builtin_specs().get(*index)
            && spec.beskid_path == ["__array_new"]
            && let Some(elem_size) = call.node.args.first().and_then(|arg| integer_literal_value(&arg.node))
            && elem_size == 1
            && let Some(u8_arr) = self.u8_array_type_id()
        {
            return_type = u8_arr;
        }

        Some(return_type)
    }
}

fn integer_literal_value(expression: &Expression) -> Option<i64> {
    let Expression::Literal(literal) = expression else {
        return None;
    };
    let Literal::Integer(text) = &literal.node.literal.node else {
        return None;
    };
    integer_literal_magnitude(text).parse().ok()
}
