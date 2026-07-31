use super::*;

impl NodeFacts for SyntaxNodeFacts<'_> {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if self.query(aggregate_field_access(self.db, key)).is_some() {
            return Some(NodeKind::FieldExpression);
        }
        if self.query(range_for_fact(self.db, key)).is_some() {
            return Some(NodeKind::RangeExpression);
        }
        self.query(node_kind(self.db, key)).and_then(map_node_kind)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.literal(key).map(|fact| match fact {
            LiteralFact::Integer(_) => LiteralKind::Integer,
            LiteralFact::Float(_) => LiteralKind::Float,
            LiteralFact::String(_) => LiteralKind::String,
            LiteralFact::Char(_) => LiteralKind::Char,
            LiteralFact::Bool(_) => LiteralKind::Boolean,
        })
    }

    fn constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        self.query(constant_integer(self.db, key))
    }

    fn canonical_runtime_constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        if self.node_kind(key) != Some(NodeKind::PathExpression) || self.input.runtime_intrinsic_capability().is_none()
        {
            return None;
        }
        self.query(constant_integer(self.db, key))
    }

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        let operator = self.query(operator_fact(self.db, key))?;
        let left_is_string = self.child(key, 0).is_some_and(|operand| self.contextual_expression_is_string(operand));
        let right_is_string = self.child(key, 1).is_some_and(|operand| self.contextual_expression_is_string(operand));
        let specialized_string_operands =
            matches!(operator, beskid_queries::OperatorFact::Eq | beskid_queries::OperatorFact::NotEq)
                && left_is_string
                && right_is_string;
        Some(match operator {
            beskid_queries::OperatorFact::Add if left_is_string || right_is_string => OperatorFact::StringAdd,
            beskid_queries::OperatorFact::Eq if specialized_string_operands => OperatorFact::StringEq,
            beskid_queries::OperatorFact::NotEq if specialized_string_operands => OperatorFact::StringNotEq,
            operator => map_operator_fact(operator),
        })
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        let children = if self.node_kind(key) == Some(NodeKind::TestDefinition) {
            self.query(test_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BlockExpression) {
            self.query(block_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BinaryExpression) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(self.query(node_kind(self.db, *child)), Some(beskid_queries::IndexedNodeKind::BinaryOp))
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::UnaryExpression) {
            // Operand-only view: UnaryOp is selected via `operator_fact`, not as a child.
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(self.query(node_kind(self.db, *child)), Some(beskid_queries::IndexedNodeKind::UnaryOp))
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::ForStatement) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    self.query(node_kind(self.db, *child)) != Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .collect()
        } else {
            self.children(key).into()
        };
        children.get(usize::from(index)).copied().and_then(|child| self.unwrap_transparent(child))
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        let kind = self.node_kind(key)?;
        match kind {
            NodeKind::BlockExpression => {
                let nodes = self.query(block_statement_nodes(self.db, key))?;
                let len =
                    if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::BlockExpression) {
                        nodes.len().saturating_sub(1)
                    } else {
                        nodes.len()
                    };
                if len > u8::MAX as usize {
                    return None;
                }
                u8::try_from(len).ok()
            }
            NodeKind::TestDefinition => {
                let nodes = self.query(test_statement_nodes(self.db, key))?;
                let len = nodes.len();
                if len > u8::MAX as usize {
                    return None;
                }
                u8::try_from(len).ok()
            }
            _ => None,
        }
    }

    fn block_result(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::BlockExpression)).then_some(())?;
        let statement = self.query(block_statement_nodes(self.db, key))?.last().copied()?;
        (self.query(node_kind(self.db, statement)) == Some(beskid_queries::IndexedNodeKind::ExpressionStatement))
            .then_some(())?;
        self.raw_children(statement).into_iter().find_map(|child| self.unwrap_transparent(child))
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        if let Some(fact) = self.query(typed_let_call_result(self.db, key)) {
            return Some(fact.initializer);
        }
        if let Some(initializer) = self.query(semantic_let_initializer(self.db, key)) {
            return Some(initializer);
        }
        (self.node_kind(key) == Some(NodeKind::LetStatement))
            .then(|| self.children(key).last().copied())?
            .and_then(|initializer| self.unwrap_transparent(initializer))
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        match self.query(node_kind(self.db, key))? {
            beskid_queries::IndexedNodeKind::PathExpression => {
                let declaration = self
                    .query(resolved_local(self.db, key))
                    .map(|resolved| resolved.declaration)
                    .or_else(|| self.query(nominal_member_receiver(self.db, key)))?;
                self.query(local_slot(self.db, declaration))
                    .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
            }
            beskid_queries::IndexedNodeKind::LetStatement => self
                .raw_children(key)
                .into_iter()
                .find(|child| {
                    self.query(node_kind(self.db, *child)) == Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .and_then(|identifier| self.query(local_slot(self.db, identifier)))
                .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index }),
            beskid_queries::IndexedNodeKind::ForStatement => self
                .query(for_iterator_fact(self.db, key))
                .and_then(|fact| self.query(local_slot(self.db, fact.declaration)))
                .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index }),
            _ => None,
        }
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.query(mutable_local_assignment(self.db, key))
            .map(|assignment| LocalSlotId { owner_node: assignment.slot.owner.node.0, index: assignment.slot.index })
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        if self.query(beskid_queries::primitive_numeric_conversion(self.db, key)).is_some() {
            return Some(CallKind::PrimitiveNumericConversion);
        }
        if self.runtime_intrinsic(key).is_some() {
            return Some(CallKind::RuntimeIntrinsic);
        }
        if self.query(dispatch_builtin_symbol(self.db, key)).is_some() {
            return Some(CallKind::Dynamic);
        }
        if self.inline_lambda_call(key).is_some() {
            return Some(CallKind::InlineLambda);
        }
        matches!(
            self.query(call_lowering(self.db, key)),
            Some(CallLowering::Direct(_) | CallLowering::ManifestBuiltin(_) | CallLowering::CorelibService(_))
        )
        .then_some(CallKind::Direct)
    }

    fn primitive_numeric_conversion(&self, key: AstNodeKey) -> Option<(SemanticTypeId, SemanticTypeId)> {
        self.query(beskid_queries::primitive_numeric_conversion(self.db, key)).map(|fact| (fact.from, fact.to))
    }

    fn dispatch_builtin_symbol(&self, key: AstNodeKey) -> Option<&'static str> {
        self.query(dispatch_builtin_symbol(self.db, key)).map(|symbol| symbol.0)
    }

    fn expression_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.contextual_expression_is_string(key)
            .then_some(SemanticTypeId::STRING)
            .or_else(|| self.specialized_direct_parameter_type(key))
            .or_else(|| self.scalar_semantic_type(key))
            .or_else(|| self.query(call_abi_signature(self.db, key)).map(|signature| signature.result))
            .or_else(|| {
                let (item, item_specialization) = self.item_specializations.iter().next()?;
                self.query(generic_call_specialization_in_item(self.db, key, *item, item_specialization))
                    .map(|specialization| specialization.signature.result)
            })
    }

    fn index_target_is_string(&self, key: AstNodeKey) -> bool {
        self.child(key, 0).and_then(|target| self.scalar_semantic_type(target)) == Some(SemanticTypeId::STRING)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        let (_, intrinsic) = self.runtime_intrinsic(key)?;
        match intrinsic.name.as_str() {
            "arch_context_size" => {
                return Some(RuntimeIntrinsicKind::ArchContextSize(self.input.target_context_layout()?.size));
            }
            "arch_context_alignment" => {
                return Some(RuntimeIntrinsicKind::ArchContextAlignment(self.input.target_context_layout()?.alignment));
            }
            _ => {}
        }
        runtime_intrinsic_kind_for_name(intrinsic.name.as_str())
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        if let Some((index, _)) = self.runtime_intrinsic(key) {
            return Some(DirectCallee::runtime_intrinsic(index));
        }
        let lowering = self.query(call_lowering(self.db, key))?;
        if let CallLowering::ManifestBuiltin(symbol) = lowering {
            return Some(DirectCallee::manifest_builtin(symbol));
        }
        if let CallLowering::CorelibService(service) = lowering {
            self.input.corelib_service_capability()?;
            return Some(DirectCallee::corelib_service(service.symbol));
        }
        let CallLowering::Direct(declaration) = lowering else {
            return None;
        };
        let specialization = self
            .query(generic_call_specialization(self.db, key))
            .or_else(|| self.query(explicit_generic_call_specialization(self.db, key)))
            .or_else(|| {
                let (item, item_specialization) = self.item_specializations.iter().next()?;
                self.query(generic_call_specialization_in_item(self.db, key, *item, item_specialization))
            });
        if let Some(specialization) = specialization {
            return Some(DirectCallee::specialized_item(
                specialization.declaration,
                specialization_identity(&specialization),
            ));
        }
        Some(DirectCallee::item(declaration))
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        if let Some((_, intrinsic)) = self.runtime_intrinsic(key) {
            return signature_for_runtime_intrinsic(self.isa?, intrinsic);
        }
        let signature = self
            .query(call_abi_signature(self.db, key))
            .or_else(|| self.query(explicit_generic_call_specialization(self.db, key)).map(|fact| fact.signature))
            .or_else(|| {
                let (item, item_specialization) = self.item_specializations.iter().next()?;
                self.query(generic_call_specialization_in_item(self.db, key, *item, item_specialization))
                    .map(|specialization| specialization.signature)
            })?;
        signature_for_item(self.isa?, signature)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.query(call_arguments(self.db, key))
            .and_then(|arguments| arguments.iter().copied().map(|argument| self.unwrap_transparent(argument)).collect())
    }

    fn inline_lambda_call(&self, key: AstNodeKey) -> Option<InlineLambdaCall> {
        let target = self.query(closure_call_target(self.db, key))?;
        let environment = self.query(closure_environment(self.db, target.lambda))?;
        if environment.parameters.len() != target.callable.parameters.len() {
            return None;
        }
        let closure_environment = if environment.captures.is_empty() {
            None
        } else {
            Some(self.inline_closure_environment(key, target.lambda)?)
        };
        let parameters = environment
            .parameters
            .iter()
            .copied()
            .zip(target.callable.parameters.iter().copied())
            .map(|(parameter, semantic)| {
                let slot = self.query(local_slot(self.db, parameter))?;
                Some(ParameterSlot {
                    slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                    value_type: map_signature_type(self.isa?, semantic)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(InlineLambdaCall {
            body: target.body,
            parameters,
            result_type: map_signature_type(self.isa?, target.callable.result)?,
            closure_environment,
        })
    }

    fn array_elements(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.array_elements_for_literal(key)
    }

    fn array_layout(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        self.array_layout_for_literal(key)
    }

    fn runtime_array_layout(&self, key: AstNodeKey) -> Option<RuntimeArrayLayout> {
        if self.node_kind(key) != Some(NodeKind::IndexExpression) || self.index_target_is_string(key) {
            return None;
        }
        let pointer = self.isa?.pointer_type();
        Some(RuntimeArrayLayout::new(
            self.scalar_type(key).unwrap_or(pointer),
            pointer.bytes(),
            0,
            pointer.bytes() as i32,
        ))
    }

    fn function_parameters(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        let mut parameters = Vec::new();
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::MethodDefinition) {
            parameters.push(ParameterSlot {
                // Methods cannot spell `self` in Beskid source. The ABI receiver still needs a
                // materialized local so its declared pointer position is consumed by ISLE.
                slot: LocalSlotId { owner_node: u32::MAX, index: u32::MAX },
                value_type: self.isa?.pointer_type(),
            });
        }
        self.collect_function_parameters(key, &mut parameters)?;
        Some(parameters)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        let LiteralFact::Integer(text) = self.literal(key)? else {
            return None;
        };
        let value = text.split_once('_').map_or(text.as_ref(), |(value, _)| value);
        match value.strip_prefix("0x") {
            Some(hexadecimal) => u64::from_str_radix(hexadecimal, 16).ok().map(|number| number as i64),
            None => value.parse().ok(),
        }
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        match self.literal(key)? {
            LiteralFact::Bool(value) => Some(value),
            _ => None,
        }
    }

    fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
        let LiteralFact::Float(text) = self.literal(key)? else {
            return None;
        };
        text.parse().ok()
    }

    fn char_literal(&self, key: AstNodeKey) -> Option<char> {
        let LiteralFact::Char(text) = self.literal(key)? else {
            return None;
        };
        text.trim_matches('\'').chars().next()
    }

    fn string_literal(&self, key: AstNodeKey) -> Option<std::sync::Arc<str>> {
        let LiteralFact::String(text) = self.literal(key)? else {
            return None;
        };
        try_decode_string_literal_token(&text).map(Into::into)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        if self.contextual_expression_is_string(key) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::AssignExpression) {
            return self.child(key, 0).and_then(|target| self.scalar_type(target));
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::BlockExpression) {
            return self.block_result(key).and_then(|result| self.scalar_type(result));
        }
        if self.node_kind(key) == Some(NodeKind::LetStatement) {
            let initializer = self.let_initializer(key)?;
            if let Some(semantic) = self.item_specializations.iter().find_map(|(item, specialization)| {
                self.query(generic_call_specialization_in_item(self.db, initializer, *item, specialization))
                    .map(|specialization| specialization.signature.result)
            }) {
                return map_signature_type(self.isa?, semantic);
            }
            if let Some(semantic) = self.scalar_semantic_type(key) {
                return map_signature_type(self.isa?, semantic);
            }
            return self.scalar_type(initializer);
        }
        if let Some(fact) = self.query(typed_let_call_result(self.db, key)) {
            return map_signature_type(self.isa?, fact.abi_type);
        }
        if self.node_kind(key) == Some(NodeKind::MatchExpression) {
            let arm = self.match_arms(key).or_else(|| self.scalar_match_arms(key))?.into_iter().next()?;
            return self.scalar_type(arm.body());
        }
        if self.node_kind(key) == Some(NodeKind::StructLiteralExpression)
            && self.query(aggregate_literal_declaration(self.db, key)).is_some()
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::EnumLiteralExpression)
            && (self.query(enum_constructor(self.db, key)).is_some()
                || self.item_specializations.iter().any(|(item, specialization)| {
                    self.query(enum_constructor_in_item(self.db, key, *item, specialization)).is_some()
                }))
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        let semantic = self
            .query(call_argument_abi_type(self.db, key))
            .or_else(|| self.scalar_semantic_type(key))
            .or_else(|| self.query(call_abi_signature(self.db, key)).map(|signature| signature.result))
            .or_else(|| {
                self.query(explicit_generic_call_specialization(self.db, key)).map(|fact| fact.signature.result)
            })
            .or_else(|| {
                let (item, item_specialization) = self.item_specializations.iter().next()?;
                self.query(generic_call_specialization_in_item(self.db, key, *item, item_specialization))
                    .map(|specialization| specialization.signature.result)
            })?;
        if matches!(semantic, SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        map_scalar_type(semantic)
    }

    fn struct_fields(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::StructLiteralExpression)).then(|| {
            self.raw_children(key)
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child)) == Some(beskid_queries::IndexedNodeKind::StructLiteralField)
                })
                .filter_map(|field| self.raw_children(field).last().copied())
                .filter_map(|value| self.unwrap_transparent(value))
                .collect()
        })
    }

    fn struct_layout(&self, key: AstNodeKey) -> Option<StructLayout> {
        self.struct_layout_for_literal(key).or_else(|| {
            self.query(aggregate_field_access(self.db, key))
                .and_then(|access| self.struct_layout_for_declaration(access.declaration))
        })
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        Some(ManagedStructAllocation {
            allocation_request_symbol: self
                .input
                .aggregate_static_plan(key)
                .or_else(|| self.input.enum_static_plan(key))
                .or_else(|| {
                    self.item_specializations.iter().find_map(|(item, specialization)| {
                        self.input.enum_static_plan_in_item(key, *item, specialization)
                    })
                })?
                .allocation_request_symbol
                .into(),
        })
    }

    fn field_index(&self, key: AstNodeKey) -> Option<u32> {
        self.query(aggregate_field_access(self.db, key)).map(|access| access.index)
    }

    fn field_receiver_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        let access = self.query(aggregate_field_access(self.db, key))?;
        self.query(local_slot(self.db, access.receiver))
            .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
    }

    fn enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.enum_layout_for(key)
    }

    fn enum_variant_index(&self, key: AstNodeKey) -> Option<u32> {
        self.query(enum_constructor(self.db, key))
            .or_else(|| {
                self.item_specializations.iter().find_map(|(item, specialization)| {
                    self.query(enum_constructor_in_item(self.db, key, *item, specialization))
                        .map(|(constructor, _)| constructor)
                })
            })
            .map(|constructor| constructor.variant_index)
    }

    fn enum_payloads(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        let constructor = self.query(enum_constructor(self.db, key)).or_else(|| {
            self.item_specializations.iter().find_map(|(item, specialization)| {
                self.query(enum_constructor_in_item(self.db, key, *item, specialization))
                    .map(|(constructor, _)| constructor)
            })
        })?;
        Some(constructor.payloads.to_vec())
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        let fact = self.query(enum_match(self.db, key))?;
        fact.arms
            .iter()
            .map(|arm| {
                let bindings = arm
                    .bindings
                    .iter()
                    .map(|binding| match binding {
                        None => Some(None),
                        Some(binding) => {
                            let slot = self.query(local_slot(self.db, binding.declaration))?;
                            let value_type = match binding.payload {
                                AggregateFieldShape::Scalar(semantic) => map_signature_type(self.isa?, semantic)?,
                                AggregateFieldShape::Nominal(_) => self.isa?.pointer_type(),
                            };
                            Some(Some(MatchArmBindingFact {
                                slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                                value_type,
                            }))
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(match arm.variant_index {
                    Some(variant) => MatchArmFact::variant_with_bindings(u64::from(variant), arm.body, bindings),
                    None if bindings.is_empty() => MatchArmFact::wildcard(arm.body),
                    None => return None,
                })
            })
            .collect()
    }

    fn scalar_match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        self.query(scalar_match(self.db, key))?
            .arms
            .iter()
            .map(|arm| {
                Some(match arm.discriminant {
                    Some(discriminant) => MatchArmFact::variant(discriminant, arm.body),
                    None => MatchArmFact::wildcard(arm.body),
                })
            })
            .collect()
    }

    fn range_fact(&self, key: AstNodeKey) -> Option<beskid_isle::RangeFact> {
        let range = self.query(range_for_fact(self.db, key))?;
        Some(beskid_isle::RangeFact::new(range.start, range.end, 1, false))
    }

    fn spawn_entry(&self, key: AstNodeKey) -> Option<beskid_isle::SpawnEntry> {
        let validation = self.query(spawn_entry_validation(self.db, key))?;
        if !validation.is_zero_argument_entry {
            return None;
        }
        let closure_environment = match self.query(node_kind(self.db, validation.target))? {
            beskid_queries::IndexedNodeKind::PathExpression => {
                let _target = self.query(resolved_item(self.db, validation.target))?;
                None
            }
            beskid_queries::IndexedNodeKind::LambdaExpression => {
                let environment = self.query(closure_environment(self.db, validation.target))?;
                if environment.captures.is_empty() {
                    None
                } else {
                    Some(self.inline_closure_environment(key, validation.target)?)
                }
            }
            _ => return None,
        };
        Some(beskid_isle::SpawnEntry { trampoline: DirectCallee::spawn_trampoline(key), closure_environment })
    }
}
