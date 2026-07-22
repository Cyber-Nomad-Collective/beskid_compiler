//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use std::collections::HashMap;

use beskid_analysis::syntax::try_decode_string_literal_token;
use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, DirectCallee, EmissionServices, EnumLayout,
    EnumVariantLayout, FieldLayout, FunctionEmissionError, FunctionEmitter, InlineCaptureField,
    InlineClosureEnvironment, InlineLambdaCall, ItemStatementEmission, LiteralKind, LocalSlotId,
    MatchArmFact, NodeFacts, NodeKind, OperatorFact, ParameterSlot, RuntimeIntrinsicKind,
    Signature, StringInterner, StructLayout,
};
use beskid_queries::{
    AggregateFieldShape, CallLowering, Db, ItemSignature, LiteralFact, SemanticTypeId, abi_type,
    aggregate_field_access, aggregate_layout, aggregate_literal_declaration, block_statement_nodes,
    call_abi_signature, call_argument_abi_type, call_arguments, call_lowering, cast_intents,
    child_nodes, closure_call_target, closure_environment, dispatch_builtin_symbol,
    enum_constructor, enum_layout, enum_match, generic_call_specialization, item_abi_signature,
    item_body, literal_fact, local_slot, mutable_local_assignment, node_kind, node_type,
    nominal_member_receiver, operator_fact, range_for_fact, resolved_item, resolved_local,
    for_iterator_fact, runtime_intrinsic_name, spawn_entry_validation, test_statement_nodes,
};
use cranelift_codegen::ir::{FuncRef, Type, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};

use crate::CodegenInput;

/// Query-backed facts for generated ISLE selection.
///
/// Every answer is read from the generation-safe syntax authority registered by the typed
/// program. Missing or not-yet-ported facts remain unavailable to ISLE instead of falling back
/// to HIR or hand-built test facts.
pub struct SyntaxNodeFacts<'db> {
    db: &'db dyn Db,
    input: &'db CodegenInput<'db>,
    isa: Option<&'db dyn TargetIsa>,
    item_specializations: HashMap<AstNodeKey, ItemSignature>,
}

impl<'db> SyntaxNodeFacts<'db> {
    pub fn new(input: &'db CodegenInput<'db>) -> Self {
        Self {
            db: input.database(),
            input,
            isa: None,
            item_specializations: HashMap::new(),
        }
    }

    fn new_with_isa(input: &'db CodegenInput<'db>, isa: &'db dyn TargetIsa) -> Self {
        Self {
            db: input.database(),
            input,
            isa: Some(isa),
            item_specializations: HashMap::new(),
        }
    }

    fn new_with_item_specialization(
        input: &'db CodegenInput<'db>,
        isa: &'db dyn TargetIsa,
        item: AstNodeKey,
        signature: ItemSignature,
    ) -> Self {
        Self {
            db: input.database(),
            input,
            isa: Some(isa),
            item_specializations: HashMap::from([(item, signature)]),
        }
    }

    fn query<T>(&self, result: beskid_queries::SemanticQueryResult<T>) -> Option<T> {
        result.ok().flatten()
    }

    fn inline_closure_environment(
        &self,
        site: AstNodeKey,
        lambda: AstNodeKey,
    ) -> Option<InlineClosureEnvironment> {
        let isa = self.isa?;
        let authority = self.input.closure_lowering_authority(site, lambda)?;
        let captures = authority
            .plan
            .captures
            .iter()
            .map(|field| {
                Some(InlineCaptureField {
                    local_slot: LocalSlotId {
                        owner_node: field.capture.slot.owner.node.0,
                        index: field.capture.slot.index,
                    },
                    field_offset: u32::try_from(field.field_offset).ok()?,
                    pointer_map_index: field.pointer_map_index,
                    value_type: map_signature_type(isa, field.abi_type)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(InlineClosureEnvironment {
            allocation_request_symbol: authority.plan.allocation_request_symbol.into(),
            descriptor_symbol: authority.plan.descriptor_symbol.into(),
            root_slot_index: authority.root.slot_index,
            captures,
        })
    }

    fn specialized_direct_parameter_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        (self.query(node_kind(self.db, key))
            == Some(beskid_queries::IndexedNodeKind::PathExpression))
        .then_some(())?;
        let declaration = self.query(resolved_local(self.db, key))?.declaration;
        let slot = self.query(local_slot(self.db, declaration))?;
        self.item_specializations
            .get(&slot.owner)?
            .parameters
            .get(usize::try_from(slot.index).ok()?)
            .copied()
    }
}

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

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        let operator = self.query(operator_fact(self.db, key))?;
        let specialized_string_operands = matches!(
            operator,
            beskid_queries::OperatorFact::Eq | beskid_queries::OperatorFact::NotEq
        ) && self
            .child(key, 0)
            .and_then(|operand| self.specialized_direct_parameter_type(operand))
            == Some(SemanticTypeId::STRING)
            && self
                .child(key, 1)
                .and_then(|operand| self.specialized_direct_parameter_type(operand))
                == Some(SemanticTypeId::STRING);
        Some(match (operator, specialized_string_operands) {
            (beskid_queries::OperatorFact::Eq, true) => OperatorFact::StringEq,
            (beskid_queries::OperatorFact::NotEq, true) => OperatorFact::StringNotEq,
            (operator, _) => map_operator_fact(operator),
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
                    !matches!(
                        self.query(node_kind(self.db, *child)),
                        Some(beskid_queries::IndexedNodeKind::BinaryOp)
                    )
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::UnaryExpression) {
            // Operand-only view: UnaryOp is selected via `operator_fact`, not as a child.
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(
                        self.query(node_kind(self.db, *child)),
                        Some(beskid_queries::IndexedNodeKind::UnaryOp)
                    )
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::ForStatement) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    self.query(node_kind(self.db, *child))
                        != Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .collect()
        } else {
            self.children(key).into()
        };
        children
            .get(usize::from(index))
            .copied()
            .and_then(|child| self.unwrap_transparent(child))
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        matches!(
            self.node_kind(key),
            Some(NodeKind::BlockExpression | NodeKind::TestDefinition)
        )
        .then(|| {
            let length = if self.node_kind(key) == Some(NodeKind::TestDefinition) {
                let nodes = self.query(test_statement_nodes(self.db, key))?;
                nodes.len()
            } else if self.node_kind(key) == Some(NodeKind::BlockExpression) {
                let nodes = self.query(block_statement_nodes(self.db, key))?;
                nodes.len()
            } else {
                self.children(key).len()
            };
            u8::try_from(length).ok()
        })?
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
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
                    .map(|slot| LocalSlotId {
                        owner_node: slot.owner.node.0,
                        index: slot.index,
                    })
            }
            beskid_queries::IndexedNodeKind::LetStatement => self
                .raw_children(key)
                .into_iter()
                .find(|child| {
                    self.query(node_kind(self.db, *child))
                        == Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .and_then(|identifier| self.query(local_slot(self.db, identifier)))
                .map(|slot| LocalSlotId {
                    owner_node: slot.owner.node.0,
                    index: slot.index,
                }),
            beskid_queries::IndexedNodeKind::ForStatement => self
                .query(for_iterator_fact(self.db, key))
                .and_then(|fact| self.query(local_slot(self.db, fact.declaration)))
                .map(|slot| LocalSlotId {
                    owner_node: slot.owner.node.0,
                    index: slot.index,
                }),
            _ => None,
        }
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.query(mutable_local_assignment(self.db, key))
            .map(|assignment| LocalSlotId {
                owner_node: assignment.slot.owner.node.0,
                index: assignment.slot.index,
            })
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
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
            Some(CallLowering::Direct(_) | CallLowering::CorelibService(_))
        )
        .then_some(CallKind::Direct)
    }

    fn dispatch_builtin_symbol(&self, key: AstNodeKey) -> Option<&'static str> {
        self.query(dispatch_builtin_symbol(self.db, key))
            .map(|symbol| symbol.0)
    }

    fn expression_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.specialized_direct_parameter_type(key)
            .or_else(|| self.scalar_semantic_type(key))
    }

    fn index_target_is_string(&self, key: AstNodeKey) -> bool {
        self.child(key, 0)
            .and_then(|target| self.scalar_semantic_type(target))
            == Some(SemanticTypeId::STRING)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        let (_, intrinsic) = self.runtime_intrinsic(key)?;
        runtime_intrinsic_kind_for_name(intrinsic.name.as_str())
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        if let Some((index, _)) = self.runtime_intrinsic(key) {
            return Some(DirectCallee::runtime_intrinsic(index));
        }
        let lowering = self.query(call_lowering(self.db, key))?;
        if let CallLowering::CorelibService(service) = lowering {
            self.input.corelib_service_capability()?;
            return Some(DirectCallee::corelib_service(service.symbol));
        }
        let CallLowering::Direct(declaration) = lowering else {
            return None;
        };
        if let Some(specialization) = self.query(generic_call_specialization(self.db, key)) {
            return Some(DirectCallee::specialized_item(
                specialization.declaration,
                specialization_identity(&specialization.signature),
            ));
        }
        Some(DirectCallee::item(declaration))
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        if let Some((_, intrinsic)) = self.runtime_intrinsic(key) {
            return signature_for_runtime_intrinsic(self.isa?, intrinsic);
        }
        signature_for_item(self.isa?, self.query(call_abi_signature(self.db, key))?)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.query(call_arguments(self.db, key))
            .and_then(|arguments| {
                arguments
                    .iter()
                    .copied()
                    .map(|argument| self.unwrap_transparent(argument))
                    .collect()
            })
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
                    slot: LocalSlotId {
                        owner_node: slot.owner.node.0,
                        index: slot.index,
                    },
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

    fn function_parameters(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        let mut parameters = Vec::new();
        if self.query(node_kind(self.db, key))
            == Some(beskid_queries::IndexedNodeKind::MethodDefinition)
        {
            parameters.push(ParameterSlot {
                // Methods cannot spell `self` in Beskid source. The ABI receiver still needs a
                // materialized local so its declared pointer position is consumed by ISLE.
                slot: LocalSlotId {
                    owner_node: u32::MAX,
                    index: u32::MAX,
                },
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
        text.split_once('_')
            .map_or(text.as_ref(), |(value, _)| value)
            .parse()
            .ok()
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
        if self.node_kind(key) == Some(NodeKind::StructLiteralExpression)
            && self
                .query(aggregate_literal_declaration(self.db, key))
                .is_some()
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::EnumLiteralExpression)
            && self.query(enum_constructor(self.db, key)).is_some()
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        let semantic = self
            .query(call_argument_abi_type(self.db, key))
            .or_else(|| self.scalar_semantic_type(key))
            .or_else(|| Some(self.query(call_abi_signature(self.db, key))?.result))?;
        if matches!(
            semantic,
            SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING
        ) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        map_scalar_type(semantic)
    }

    fn struct_fields(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::StructLiteralExpression)).then(|| {
            self.raw_children(key)
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child))
                        == Some(beskid_queries::IndexedNodeKind::StructLiteralField)
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

    fn field_index(&self, key: AstNodeKey) -> Option<u32> {
        self.query(aggregate_field_access(self.db, key))
            .map(|access| access.index)
    }

    fn field_receiver_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        let access = self.query(aggregate_field_access(self.db, key))?;
        self.query(local_slot(self.db, access.receiver))
            .map(|slot| LocalSlotId {
                owner_node: slot.owner.node.0,
                index: slot.index,
            })
    }

    fn enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.enum_layout_for(key)
    }

    fn enum_variant_index(&self, key: AstNodeKey) -> Option<u32> {
        self.query(enum_constructor(self.db, key))
            .map(|constructor| constructor.variant_index)
    }

    fn enum_payload(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        self.query(enum_constructor(self.db, key))?.payload
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        self.query(enum_match(self.db, key)).map(|fact| {
            fact.arms
                .iter()
                .map(|arm| match arm.variant_index {
                    Some(variant) => MatchArmFact::variant(u64::from(variant), arm.body),
                    None => MatchArmFact::wildcard(arm.body),
                })
                .collect()
        })
    }

    fn range_fact(&self, key: AstNodeKey) -> Option<beskid_isle::RangeFact> {
        let range = self.query(range_for_fact(self.db, key))?;
        Some(beskid_isle::RangeFact::new(
            range.start,
            range.end,
            1,
            false,
        ))
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
        Some(beskid_isle::SpawnEntry {
            trampoline: DirectCallee::spawn_trampoline(key),
            closure_environment,
        })
    }
}

impl SyntaxNodeFacts<'_> {
    fn struct_layout_for_literal(&self, key: AstNodeKey) -> Option<StructLayout> {
        let declaration = self.query(aggregate_literal_declaration(self.db, key))?;
        self.struct_layout_for_declaration(declaration)
    }

    fn struct_layout_for_declaration(&self, declaration: AstNodeKey) -> Option<StructLayout> {
        let isa = self.isa?;
        let aggregate = self.query(aggregate_layout(self.db, declaration))?;
        let mut size = 0_u32;
        let mut alignment = 1_u32;
        let mut fields = Vec::with_capacity(aggregate.fields.len());
        for (_, shape) in aggregate.fields.iter() {
            let value_type = match shape {
                AggregateFieldShape::Scalar(semantic) => map_signature_type(isa, *semantic)?,
                AggregateFieldShape::Nominal(_) => isa.pointer_type(),
            };
            let field_alignment = value_type.bytes().next_power_of_two();
            size = align_to(size, field_alignment)?;
            fields.push(FieldLayout::new(value_type, size));
            size = size.checked_add(value_type.bytes())?;
            alignment = alignment.max(field_alignment);
        }
        // An empty nominal value still needs an addressable ABI-v5 reference. Keep its source
        // layout empty while reserving one byte for the stack-backed literal representation.
        let size = align_to(size, alignment)?.max(1);
        Some(StructLayout::new(size, alignment.ilog2() as u8, fields))
    }

    fn enum_layout_for(&self, key: AstNodeKey) -> Option<EnumLayout> {
        let isa = self.isa?;
        let layout_key = if self.query(enum_constructor(self.db, key)).is_some() {
            key
        } else {
            // Generic match instantiation deliberately remains unavailable here: current match
            // facts carry only a declaration key. CYB-137 owns the applied scrutinee layout.
            self.query(enum_match(self.db, key))?.declaration
        };
        let source = self.query(enum_layout(self.db, layout_key))?;
        let tag_type = types::I32;
        let tag = FieldLayout::new(tag_type, 0);
        let mut alignment = tag_type.bytes();
        let mut payload_offset = tag_type.bytes();
        let mut variants = Vec::with_capacity(source.variants.len());
        let mut payloads = Vec::with_capacity(source.variants.len());
        for variant in source.variants.iter() {
            let payload = match variant.fields.as_ref() {
                [] => None,
                [(_, AggregateFieldShape::Scalar(semantic))] => {
                    Some(map_signature_type(isa, *semantic)?)
                }
                [(_, AggregateFieldShape::Nominal(_))] => Some(isa.pointer_type()),
                _ => return None,
            };
            if let Some(payload) = payload {
                alignment = alignment.max(payload.bytes());
                payload_offset = payload_offset.max(align_to(tag_type.bytes(), payload.bytes())?);
            }
            payloads.push(payload);
        }
        let size = payloads
            .iter()
            .flatten()
            .fold(tag_type.bytes(), |size, payload| {
                size.max(payload_offset.saturating_add(payload.bytes()))
            });
        let size = align_to(size, alignment)?.max(1);
        for (index, payload) in payloads.into_iter().enumerate() {
            variants.push(EnumVariantLayout::new(
                u64::try_from(index).ok()?,
                payload.map(|value_type| FieldLayout::new(value_type, payload_offset)),
            ));
        }
        Some(EnumLayout::new(
            size,
            alignment.ilog2() as u8,
            tag,
            variants,
        ))
    }

    fn array_elements_for_literal(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression)).then(|| {
            self.raw_children(key)
                .into_iter()
                .filter_map(|child| self.unwrap_transparent(child))
                .collect()
        })
    }

    fn array_layout_for_literal(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let elements = self.array_elements_for_literal(key)?;
        if !elements.is_empty() {
            return None;
        }
        let pointer = self.isa?.pointer_type();
        Some(beskid_isle::ArrayLayout::new(
            pointer,
            pointer.bytes(),
            0,
            pointer.bytes().ilog2() as u8,
        ))
    }
    fn runtime_intrinsic(
        &self,
        key: AstNodeKey,
    ) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.runtime_intrinsic_for(key, &name.0)
    }
    fn collect_function_parameters(
        &self,
        key: AstNodeKey,
        parameters: &mut Vec<ParameterSlot>,
    ) -> Option<()> {
        for child in self.raw_children(key) {
            match self.query(node_kind(self.db, child))? {
                beskid_queries::IndexedNodeKind::Block => continue,
                beskid_queries::IndexedNodeKind::Parameter => {
                    let identifier = self.raw_children(child).into_iter().find(|candidate| {
                        self.query(node_kind(self.db, *candidate))
                            == Some(beskid_queries::IndexedNodeKind::Identifier)
                    })?;
                    let slot = self.query(local_slot(self.db, identifier))?;
                    let specialization = self
                        .item_specializations
                        .get(&key)
                        .and_then(|signature| signature.parameters.get(parameters.len()))
                        .copied();
                    let value_type = specialization
                        .or_else(|| self.scalar_semantic_type(identifier))
                        .and_then(|semantic| {
                            if matches!(
                                semantic,
                                SemanticTypeId::WORD
                                    | SemanticTypeId::POINTER
                                    | SemanticTypeId::STRING
                            ) {
                                self.isa.map(|isa| isa.pointer_type())
                            } else {
                                map_scalar_type(semantic)
                            }
                        })?;
                    parameters.push(ParameterSlot {
                        slot: LocalSlotId {
                            owner_node: slot.owner.node.0,
                            index: slot.index,
                        },
                        value_type,
                    });
                }
                _ => self.collect_function_parameters(child, parameters)?,
            }
        }
        Some(())
    }

    fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if self.query(node_kind(self.db, key))
            == Some(beskid_queries::IndexedNodeKind::ForStatement)
        {
            return self
                .query(for_iterator_fact(self.db, key))
                .map(|fact| fact.element_type);
        }
        self.query(cast_intents(self.db, key))
            .and_then(|intents| intents.first().map(|intent| intent.to))
            .or_else(|| self.query(abi_type(self.db, key)))
            .or_else(|| self.query(node_type(self.db, key)))
            .or_else(|| {
                (self.query(node_kind(self.db, key))
                    == Some(beskid_queries::IndexedNodeKind::LetStatement))
                .then(|| {
                    self.raw_children(key)
                        .into_iter()
                        .find(|child| {
                            self.query(node_kind(self.db, *child))
                                == Some(beskid_queries::IndexedNodeKind::Identifier)
                        })
                        .and_then(|identifier| self.query(abi_type(self.db, identifier)))
                        .or_else(|| {
                            self.raw_children(key)
                                .into_iter()
                                .find(|child| {
                                    self.query(node_kind(self.db, *child))
                                        == Some(beskid_queries::IndexedNodeKind::Identifier)
                                })
                                .and_then(|identifier| self.query(node_type(self.db, identifier)))
                        })
                })?
            })
            .or_else(|| {
                self.query(aggregate_field_access(self.db, key))
                    .and_then(|access| {
                        self.query(aggregate_layout(self.db, access.declaration))?
                            .fields
                            .get(usize::try_from(access.index).ok()?)
                            .map(|(_, shape)| match shape {
                                AggregateFieldShape::Scalar(semantic) => *semantic,
                                AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                            })
                    })
            })
            .or_else(|| {
                let (_, intrinsic) = self.runtime_intrinsic(key)?;
                semantic_type_for_runtime_intrinsic(intrinsic)
            })
    }

    fn literal(&self, key: AstNodeKey) -> Option<LiteralFact> {
        self.query(literal_fact(self.db, key)).or_else(|| {
            self.query(child_nodes(self.db, key))?
                .iter()
                .find_map(|child| self.query(literal_fact(self.db, *child)))
        })
    }

    fn children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        let children = self.raw_children(key);
        let children = if self.query(node_kind(self.db, key))
            == Some(beskid_queries::IndexedNodeKind::TestDefinition)
        {
            children
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child))
                        == Some(beskid_queries::IndexedNodeKind::Statement)
                })
                .collect()
        } else {
            children
        };
        children
            .into_iter()
            .filter_map(|child| self.unwrap_transparent(child))
            .collect()
    }

    fn raw_children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        self.query(child_nodes(self.db, key))
            .as_deref()
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    fn unwrap_transparent(&self, mut key: AstNodeKey) -> Option<AstNodeKey> {
        loop {
            let kind = self.query(node_kind(self.db, key))?;
            // ElseBranch is a structural wrapper around Block or nested If; peel it so
            // emit_if_else can lower the concrete else arm without a HIR/Lowerable fallback.
            if !matches!(
                kind,
                beskid_queries::IndexedNodeKind::Statement
                    | beskid_queries::IndexedNodeKind::Expression
                    | beskid_queries::IndexedNodeKind::ElseBranch
            ) {
                return Some(key);
            }
            let children = self.query(child_nodes(self.db, key))?;
            key = *children.first()?;
        }
    }
}

fn align_to(value: u32, alignment: u32) -> Option<u32> {
    value
        .checked_add(alignment.checked_sub(1)?)
        .map(|value| value / alignment * alignment)
}

fn semantic_type_for_runtime_intrinsic(
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<SemanticTypeId> {
    use beskid_abi::abi_v5::AbiType;

    Some(match intrinsic.result {
        AbiType::Void => return None,
        AbiType::Pointer => SemanticTypeId::POINTER,
        AbiType::USize => SemanticTypeId::WORD,
        AbiType::I8 => SemanticTypeId::U8,
        AbiType::U8 => SemanticTypeId::U8,
        AbiType::I32 => SemanticTypeId::I32,
        AbiType::I64 => SemanticTypeId::I64,
        AbiType::F64 => SemanticTypeId::F64,
        _ => return None,
    })
}

/// Emit a capturing spawned-lambda entry that loads transfers from its environment pointer.
pub fn emit_isle_closure_lambda_entry<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
    captures: &[InlineCaptureField],
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_closure_lambda_entry_with_call_importer(
        UserFuncName::user(0, 0),
        result,
        &facts,
        body,
        captures,
        importer,
    )
}

/// Emit one parsed expanded-syntax expression through generated ISLE selection.
pub fn emit_isle_expression<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_expression(
        UserFuncName::user(0, 0),
        emitter.signature([], [result]),
        &facts,
        body,
    )
}

/// Emit one parsed expression through generated ISLE selection with exact artifact call imports.
///
/// This is used for syntax-owned helper entries such as capture-free spawned lambdas. The caller
/// supplies the helper ABI; every nested direct call still resolves through the module's exact
/// syntax-owned symbol table rather than a legacy lowering path.
pub fn emit_isle_expression_with_call_importer<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_expression_with_call_importer(
        UserFuncName::user(0, 0),
        emitter.signature([], [result]),
        &facts,
        body,
        importer,
    )
}

/// Emit a parsed item body through generated ISLE statement selection.
///
/// Parameter materialization is derived from generation-safe local syntax facts, so this remains
/// independent of the legacy HIR lowering context.
pub fn emit_isle_item<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::verification(
                item,
                "item signature is unavailable to syntax-only ISLE emission".to_owned(),
            )
        })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement(UserFuncName::user(0, 0), signature, &facts, item, body)
}

/// Read the generation-safe syntax signature required to predeclare an item in a module.
pub fn syntax_item_signature(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<Signature, FunctionEmissionError> {
    item_abi_signature(input.database(), item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))
}

/// Emit a syntax-only item with an explicit semantic-call importer.
pub fn emit_isle_item_with_call_importer<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_call_importer(
        UserFuncName::user(0, 0),
        signature,
        &facts,
        item,
        body,
        importer,
    )
}

/// Emit a syntax-only item with the shared artifact string pool and exact call imports.
pub fn emit_isle_item_with_services<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    string_interner: &mut dyn StringInterner,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_services(
        ItemStatementEmission {
            name: UserFuncName::user(0, 0),
            signature,
            facts: &facts,
            item,
            body,
        },
        EmissionServices {
            string_interner: Some(string_interner),
            call_importer: Some(importer),
        },
    )
}

/// Emit one source item using an exact call-derived generic ABI specialization.
///
/// This is intentionally separate from [`emit_isle_item_with_services`]: ordinary declarations
/// continue to obtain their ABI from their own syntax, while generic declarations can only enter
/// through a current call fact that proves every substituted ABI type.
pub fn emit_isle_item_with_services_specialization<'db>(
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    item: AstNodeKey,
    specialization: ItemSignature,
    string_interner: &mut dyn StringInterner,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = signature_for_item(isa, specialization.clone()).ok_or_else(|| {
        FunctionEmissionError::verification(item, "generic item specialization is unavailable")
    })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_item_specialization(input, isa, item, specialization);
    emitter.emit_item_statement_with_services(
        ItemStatementEmission {
            name: UserFuncName::user(0, 0),
            signature,
            facts: &facts,
            item,
            body,
        },
        EmissionServices {
            string_interner: Some(string_interner),
            call_importer: Some(importer),
        },
    )
}

/// Explicit module importer keyed by syntax-resolved item identity.
///
/// Call lowering never guesses symbols: the host declares each item and supplies its exact
/// [`FuncId`] keyed by [`DirectCallee`].
pub struct ItemModuleImporter<'module, M: Module> {
    module: &'module mut M,
    functions: HashMap<DirectCallee, FuncId>,
}

impl<'module, M: Module> ItemModuleImporter<'module, M> {
    pub fn new(module: &'module mut M, functions: HashMap<DirectCallee, FuncId>) -> Self {
        Self { module, functions }
    }
}

impl<M: Module> CallImporter for ItemModuleImporter<'_, M> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        _signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let function = self
            .functions
            .get(&callee)
            .copied()
            .ok_or(beskid_isle::CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
}

fn signature_for_item(isa: &dyn TargetIsa, item: ItemSignature) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = item
        .parameters
        .iter()
        .copied()
        .map(|semantic| map_signature_type(isa, semantic))
        .collect::<Option<Vec<_>>>()?;
    let returns = match item.result {
        SemanticTypeId::UNIT | SemanticTypeId::NEVER => Vec::new(),
        result => vec![map_signature_type(isa, result)?],
    };
    Some(emitter.signature(parameters, returns))
}

fn specialization_identity(signature: &ItemSignature) -> std::sync::Arc<[u32]> {
    signature
        .parameters
        .iter()
        .map(|semantic| semantic.0)
        .chain(std::iter::once(signature.result.0))
        .collect::<Vec<_>>()
        .into()
}

fn signature_for_runtime_intrinsic(
    isa: &dyn TargetIsa,
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = intrinsic
        .params
        .iter()
        .copied()
        .map(|ty| map_abi_type(isa, ty))
        .collect::<Option<Vec<_>>>()?;
    let returns = if intrinsic.noreturn || intrinsic.result == beskid_abi::abi_v5::AbiType::Void {
        Vec::new()
    } else {
        vec![map_abi_type(isa, intrinsic.result)?]
    };
    Some(emitter.signature(parameters, returns))
}

fn map_node_kind(kind: beskid_queries::IndexedNodeKind) -> Option<NodeKind> {
    match beskid_isle::classify_syntax_node_kind(kind) {
        beskid_isle::SyntaxNodeClassification::IsleLowered(kind) => Some(kind),
        beskid_isle::SyntaxNodeClassification::Structural
        | beskid_isle::SyntaxNodeClassification::UnsupportedTypedOperation => None,
    }
}

fn runtime_intrinsic_kind_for_name(name: &str) -> Option<RuntimeIntrinsicKind> {
    Some(match name {
        "memory_copy" => RuntimeIntrinsicKind::MemoryCopy,
        "memory_set" => RuntimeIntrinsicKind::MemorySet,
        "native_word_from_pointer" => RuntimeIntrinsicKind::NativeWordFromPointer,
        "pointer_from_native_word" => RuntimeIntrinsicKind::PointerFromNativeWord,
        "pointer_add" => RuntimeIntrinsicKind::PointerAdd,
        "raw_word_load" => RuntimeIntrinsicKind::RawWordLoad,
        "raw_word_store" => RuntimeIntrinsicKind::RawWordStore,
        "raw_byte_load" => RuntimeIntrinsicKind::RawByteLoad,
        "raw_byte_store" => RuntimeIntrinsicKind::RawByteStore,
        _ => return None,
    })
}

fn map_operator_fact(operator: beskid_queries::OperatorFact) -> OperatorFact {
    use beskid_queries::OperatorFact as Syntax;

    match operator {
        Syntax::Or => OperatorFact::Or,
        Syntax::And => OperatorFact::And,
        Syntax::IdentityEq => OperatorFact::IdentityEq,
        Syntax::IdentityNotEq => OperatorFact::IdentityNotEq,
        Syntax::Eq => OperatorFact::Eq,
        Syntax::NotEq => OperatorFact::NotEq,
        Syntax::Lt => OperatorFact::Lt,
        Syntax::Lte => OperatorFact::Lte,
        Syntax::Gt => OperatorFact::Gt,
        Syntax::Gte => OperatorFact::Gte,
        Syntax::Add => OperatorFact::Add,
        Syntax::Sub => OperatorFact::Sub,
        Syntax::Mul => OperatorFact::Mul,
        Syntax::Div => OperatorFact::Div,
        Syntax::Mod => OperatorFact::Mod,
        Syntax::Neg => OperatorFact::Neg,
        Syntax::Not => OperatorFact::Not,
        Syntax::StringAdd => OperatorFact::StringAdd,
        Syntax::StringEq => OperatorFact::StringEq,
        Syntax::StringNotEq => OperatorFact::StringNotEq,
    }
}

fn map_scalar_type(semantic: SemanticTypeId) -> Option<Type> {
    Some(match semantic {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => types::I8,
        SemanticTypeId::I32 => types::I32,
        SemanticTypeId::I64 => types::I64,
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::NEVER => return None,
        SemanticTypeId::F64 => types::F64,
        SemanticTypeId::CHAR => types::I32,
        SemanticTypeId::UNIT | SemanticTypeId::STRING => return None,
        _ => return None,
    })
}

fn map_signature_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    if matches!(
        semantic,
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING
    ) {
        Some(isa.pointer_type())
    } else {
        map_scalar_type(semantic)
    }
}

fn map_abi_type(isa: &dyn TargetIsa, ty: beskid_abi::abi_v5::AbiType) -> Option<Type> {
    use beskid_abi::abi_v5::AbiType;
    Some(match ty {
        AbiType::Pointer | AbiType::USize | AbiType::ISize => isa.pointer_type(),
        AbiType::I8 | AbiType::U8 => types::I8,
        AbiType::I16 | AbiType::U16 => types::I16,
        AbiType::I32 | AbiType::U32 => types::I32,
        AbiType::I64 | AbiType::U64 => types::I64,
        AbiType::V128 => types::I8X16,
        AbiType::F32 => types::F32,
        AbiType::F64 => types::F64,
        AbiType::Void => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_runtime_intrinsic_names_map_to_exact_isle_kinds() {
        for (name, expected) in [
            ("memory_copy", RuntimeIntrinsicKind::MemoryCopy),
            ("memory_set", RuntimeIntrinsicKind::MemorySet),
            (
                "native_word_from_pointer",
                RuntimeIntrinsicKind::NativeWordFromPointer,
            ),
            (
                "pointer_from_native_word",
                RuntimeIntrinsicKind::PointerFromNativeWord,
            ),
            ("pointer_add", RuntimeIntrinsicKind::PointerAdd),
            ("raw_word_load", RuntimeIntrinsicKind::RawWordLoad),
            ("raw_word_store", RuntimeIntrinsicKind::RawWordStore),
            ("raw_byte_load", RuntimeIntrinsicKind::RawByteLoad),
            ("raw_byte_store", RuntimeIntrinsicKind::RawByteStore),
        ] {
            assert_eq!(
                beskid_isle::classify_syntax_node_kind(
                    beskid_queries::IndexedNodeKind::CallExpression,
                ),
                beskid_isle::SyntaxNodeClassification::IsleLowered(NodeKind::CallExpression),
            );
            assert_eq!(runtime_intrinsic_kind_for_name(name), Some(expected));
        }
        assert_eq!(runtime_intrinsic_kind_for_name("untrusted"), None);
    }
}
