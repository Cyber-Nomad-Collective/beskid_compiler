use crate::builtins::{BuiltinType, builtin_specs};
use crate::resolve::ResolvedValue;
use crate::syntax::Spanned;
use crate::syntax::{CallExpression, Expression, LambdaExpression, Literal, PrimitiveType, integer_literal_magnitude};
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

        // A lambda lowers to its own function; enclosing `%N` parameters are not in scope.
        let previous_clif_parameters = self.current_clif_parameters.take();
        // A block body is a statement lambda: its `return`s return from the lambda, with the
        // declared function type's result (unit without one), not from the enclosing function.
        let statement_body = matches!(lambda.node.body.node, Expression::Block(_));
        let statement_result = if statement_body {
            expected_signature
                .as_ref()
                .map(|(_, expected_return, _)| *expected_return)
                .or_else(|| self.primitive_type_id(PrimitiveType::Unit))
        } else {
            None
        };
        let previous_return_type = self.current_return_type;
        if statement_body {
            self.current_return_type = statement_result;
        }
        let return_type = self.type_expression(&lambda.node.body);
        self.current_return_type = previous_return_type;
        self.current_clif_parameters = previous_clif_parameters;
        let return_type = if statement_body { return_type.and(statement_result) } else { return_type };
        let return_type = return_type?;
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
        // Propagate the parameter's expected type into `contextual_expected_type` so that
        // expected-type-sensitive forms nested in call arguments (enum constructors of
        // generic enums, struct literals, ...) receive the substituted type arguments.
        // Lambdas are dispatched to the dedicated expected-signature path; the contextual
        // type is harmless for them since they consume the explicit parameter instead.
        let previous = self.contextual_expected_type;
        self.contextual_expected_type = Some(expected);
        let result = match &arg.node {
            Expression::Lambda(lambda) => self.type_lambda_expression_with_expected(lambda, Some(expected)),
            Expression::Grouped(grouped) => match &grouped.node.expr.node {
                Expression::Lambda(lambda) => self.type_lambda_expression_with_expected(lambda, Some(expected)),
                _ => self.type_expression(arg),
            },
            _ => self.type_expression(arg),
        };
        self.contextual_expected_type = previous;
        result
    }

    /// Types each call argument against its parameter. The last parameter of a `bulk` signature
    /// is `T[]` and takes every trailing argument, zero or more, as a `T`; lowering packs them
    /// into one array. Reports the mismatch and returns `false` when the argument count does not
    /// fit.
    fn type_call_arguments(&mut self, call: &Spanned<CallExpression>, params: &[TypeId], bulk: bool) -> bool {
        let bulk_element = match (bulk, params.last().and_then(|last| self.type_table.get(*last))) {
            (true, Some(TypeInfo::Array(element))) => Some(*element),
            _ => None,
        };
        let fixed = params.len() - usize::from(bulk_element.is_some());
        let count = call.node.args.len();
        if count < fixed || (bulk_element.is_none() && count != fixed) {
            self.errors.push(TypeError::CallArityMismatch { span: call.span, expected: fixed, actual: count });
            return false;
        }
        for (index, arg) in call.node.args.iter().enumerate() {
            let expected = params.get(index).copied().filter(|_| index < fixed).or(bulk_element);
            if let Some(expected) = expected
                && let Some(actual) = self.type_argument_with_expected(arg, expected)
            {
                self.require_same_type(arg.span, expected, actual);
            }
        }
        true
    }

    /// For generic inference, folds the trailing arguments of a `bulk` call into one array of the
    /// first trailing argument's type, so they line up with the declared `T[]` parameter. A call
    /// without trailing arguments leaves `T` uninferred.
    fn collapse_bulk_argument_types(&mut self, param_count: usize, mut arg_types: Vec<TypeId>) -> Vec<TypeId> {
        let fixed = param_count.saturating_sub(1);
        if let Some(&first) = arg_types.get(fixed) {
            arg_types.truncate(fixed);
            arg_types.push(self.type_table.intern(TypeInfo::Array(first)));
        }
        arg_types
    }

    pub(in crate::types::checker) fn type_call_expression(&mut self, call: &Spanned<CallExpression>) -> Option<TypeId> {
        if let Some((receiver_source, receiver_type, receiver_item_id, field_type)) =
            self.resolve_event_call_target(&call.node.callee)
        {
            let TypeInfo::Function { params, return_type } =
                self.type_table.get(field_type).cloned().unwrap_or(TypeInfo::Primitive(PrimitiveType::Unit))
            else {
                self.errors.push(TypeError::UnknownCallTarget { span: call.span });
                return None;
            };

            if !self.type_call_arguments(call, &params, false) {
                return Some(return_type);
            }

            if self.current_receiver_item_id != Some(receiver_item_id) {
                self.errors.push(TypeError::InvalidEventInvocationScope { span: call.span });
            }

            self.record_call_kind(call.id, CallLoweringKind::EventInvoke { receiver_source, receiver_type });
            return Some(return_type);
        }

        // `i32(x)`/`i64(x)`/`u32(x)`/`u8(x)`/`byte(x)`/`word(x)`/`f64(x)`: a primitive numeric
        // conversion call, classified by AST shape alone (kept in sync with
        // `primitive_numeric_conversion_target` in
        // `crates/beskid_queries/src/semantic_contract/calls/facts.rs`, and with
        // `resolve::resolver::is_primitive_numeric_conversion_call`, which stops name resolution
        // from treating this callee as an ordinary value reference). The callee never resolves to
        // a declared item, so it is typed here directly instead of through the item-call paths
        // below. Whether the argument's type is actually a primitive numeric type is judged by
        // `primitive_numeric_conversion` during ISLE lowering (see
        // `crates/beskid_isle/src/context/operators.rs`), which has the ABI facts to do so
        // precisely; only the argument itself is typed here.
        if let Expression::Path(path_expr) = &call.node.callee.node
            && call.node.args.len() == 1
            && let [segment] = path_expr.node.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && let Some(to) = primitive_numeric_conversion_target_type(segment.node.name.node.name.as_str())
        {
            // The ISLE lowering contract ("Explicit primitive numeric conversion lowering",
            // `openspec/changes/hir-free-isle-abi-v5-native-runtime/specs/compiler--codegen-and-ir--isle-lowering-contract/spec.md`)
            // limits conversions to exactly one primitive numeric argument. Judge that here,
            // typed, rather than letting a non-numeric argument (e.g. `i32(flag)` on a `bool`)
            // reach ISLE lowering and surface only as an opaque `MissingRuleOrFact`.
            if let Some(arg_type) = self.type_expression(&call.node.args[0])
                && !self.is_numeric(arg_type)
            {
                self.errors.push(TypeError::InvalidPrimitiveConversionArgument { span: call.node.args[0].span });
            }
            return self.primitive_type_id(to);
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
                    if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                        return Some(signature.return_type);
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
                if let Some((contract_item_id, signature)) = self.receiver_contract_method(receiver_type, method_name) {
                    if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                        return Some(signature.return_type);
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
                if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                    return Some(signature.return_type);
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
                    if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                        return Some(signature.return_type);
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

                if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                    return Some(signature.return_type);
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
            if let Some((contract_item_id, signature)) = self.receiver_contract_method(target_type, method_name) {
                if !self.type_call_arguments(call, &signature.params, signature.bulk) {
                    return Some(signature.return_type);
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
            if !self.type_call_arguments(call, &params, false) {
                return Some(return_type);
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
                        let arg_types = if signature.bulk {
                            self.collapse_bulk_argument_types(signature.params.len(), arg_types)
                        } else {
                            arg_types
                        };
                        if let Some(item_id) = callee_item_id {
                            self.record_generic_call_constraints(item_id, &arg_types, expected, call.span);
                        }
                        // A genuine per-argument type conflict (e.g. `word` vs `i64` bound to
                        // the same `T`) must fail closed with a clear diagnostic here, before
                        // falling through to `infer_generic_args_from_call_types`'s numeric
                        // widening silently picks one side and lets an inconsistent
                        // specialization reach ABI/codegen as an opaque "unavailable" failure.
                        if let Some(item_id) = callee_item_id
                            && let Some(conflict) = crate::types::inference::first_generic_parameter_conflict(
                                &self.type_table,
                                &self.generic_items,
                                &self.function_signatures,
                                item_id,
                                &arg_types,
                                &call.node.args,
                            )
                        {
                            let span = call
                                .node
                                .args
                                .get(conflict.second_arg_index)
                                .map_or(call.span, |arg| arg.span);
                            self.errors.push(TypeError::GenericParameterConflict {
                                span,
                                parameter: conflict.parameter,
                                first: conflict.first,
                                second: conflict.second,
                            });
                            return Some(signature.return_type);
                        }
                        let inferred = callee_item_id.and_then(|item_id| {
                            crate::types::inference::infer_generic_args_from_call_types(
                                &self.type_table,
                                &self.generic_items,
                                &self.function_signatures,
                                item_id,
                                &arg_types,
                            )
                        });
                        if let Some(inferred) = inferred {
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

        if let Some(item_id) = callee_item_id {
            self.check_generic_function_bounds(item_id, &substitution, call.span);
        }

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

        if let Some(kinds) = builtin_param_kinds.as_ref() {
            if call.node.args.len() != kinds.len() {
                self.errors.push(TypeError::CallArityMismatch {
                    span: call.span,
                    expected: kinds.len(),
                    actual: call.node.args.len(),
                });
                return Some(substituted_return);
            }
            for (index, (arg, kind)) in call.node.args.iter().zip(kinds.iter()).enumerate() {
                if let Some(expected) = substituted_params.get(index) {
                    // Legacy managed Corelib bridges retain their surface adapters. Raw
                    // pointer primitives have an actual type and consume their ABI slot just
                    // like every other parameter; skipping it must never shift later arguments.
                    if matches!(kind, BuiltinType::Ptr)
                        && !matches!(self.type_table.get(*expected), Some(TypeInfo::Primitive(PrimitiveType::Pointer)))
                    {
                        let _ = self.type_expression(arg);
                        continue;
                    }
                    if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                        self.require_same_type(arg.span, *expected, actual);
                    }
                }
            }
        } else if !self.type_call_arguments(call, &substituted_params, signature.bulk) {
            return Some(substituted_return);
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

/// Maps a single-segment call callee name to its primitive numeric conversion target, or `None`
/// when `name` does not name a conversion form.
///
/// Kept in sync with `primitive_numeric_conversion_target` in
/// `crates/beskid_queries/src/semantic_contract/calls/facts.rs` (the two live in separate crates
/// -- `beskid_queries` depends on `beskid_analysis`, not the reverse -- so the mapping cannot be
/// shared and must be updated together) and with
/// `resolve::resolver::PRIMITIVE_NUMERIC_CONVERSION_NAMES`.
fn primitive_numeric_conversion_target_type(name: &str) -> Option<PrimitiveType> {
    Some(match name {
        "i32" => PrimitiveType::I32,
        "i64" => PrimitiveType::I64,
        "u32" => PrimitiveType::U32,
        "u8" | "byte" => PrimitiveType::U8,
        "word" => PrimitiveType::Word,
        "f64" => PrimitiveType::F64,
        _ => return None,
    })
}

#[cfg(test)]
mod bulk_tests {
    use crate::services::{SemanticFactsError, parse_program, resolve_and_type_program};
    use crate::types::result::TypeError;

    fn type_errors(source: &str) -> Vec<TypeError> {
        let program = parse_program(source).expect("source parses");
        match resolve_and_type_program(&program) {
            Ok(_) => Vec::new(),
            Err(SemanticFactsError::Type { errors, .. }) => errors,
            Err(other) => panic!("expected type checking to run: {other:?}"),
        }
    }

    const SUM: &str = "i64 Sum(bulk i64[] values) { return values[0]; }\n\
                       T First<T>(bulk T[] values) { return values[0]; }\n\
                       i64 Tail(string label, bulk i64[] values) { return values[0]; }\n";

    #[test]
    fn bulk_parameter_takes_any_number_of_element_arguments() {
        for call in [
            "Sum()",
            "Sum(1_i64)",
            "Sum(1_i64, 2_i64, 3_i64)",
            "First<i64>(1_i64, 2_i64)",
            "First(4_i64, 5_i64)",
            "Tail(\"t\", 1_i64, 2_i64)",
            "Tail(\"t\")",
        ] {
            let source = format!("{SUM}i64 Main() {{ return {call}; }}");
            assert!(type_errors(&source).is_empty(), "{call}: {:?}", type_errors(&source));
        }
    }

    #[test]
    fn bulk_arguments_are_typed_against_the_element_type() {
        let errors = type_errors(&format!("{SUM}i64 Main() {{ return Sum(1_i64, \"two\"); }}"));
        assert!(errors.iter().any(|error| matches!(error, TypeError::TypeMismatch { .. })), "{errors:?}");
        let errors = type_errors(&format!("{SUM}i64 Main() {{ return Tail(); }}"));
        assert!(
            errors.iter().any(|error| matches!(error, TypeError::CallArityMismatch { expected: 1, actual: 0, .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn bulk_generic_without_arguments_needs_type_arguments() {
        let errors = type_errors(&format!("{SUM}i64 Main() {{ return First(); }}"));
        assert!(errors.iter().any(|error| matches!(error, TypeError::MissingTypeArguments { .. })), "{errors:?}");
    }
}
