use super::*;

impl SyntaxNodeFacts<'_> {
    /// Resolve one explicitly generic Corelib value service through the immutable specialization
    /// of the item being lowered. Pointer-shaped native values are deliberately rejected because
    /// only source identity can prove that the managed adapter is safe.
    pub(super) fn typed_corelib_value_service(
        &self,
        key: AstNodeKey,
        service: beskid_abi::runtime_source::CorelibService,
    ) -> Option<(&'static str, SemanticTypeId)> {
        let dispatch = beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service)?;
        let assembly = &self.input.typed_program().assembly;
        let unit_index =
            assembly.units.iter().position(|unit| unit.path.as_path() == key.unit.path(self.db).as_path())?;
        let program = &assembly.units[unit_index].program;
        let index = &assembly.syntax_indexes[unit_index];
        let call = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node else { return None };
        let [terminal] = callee.node.path.node.segments.as_slice() else { return None };
        let [argument] = terminal.node.type_args.as_slice() else { return None };
        let beskid_analysis::syntax::Type::Complex(argument_path) = &argument.node else { return None };
        let [parameter] = argument_path.node.segments.as_slice() else { return None };
        if !parameter.node.type_args.is_empty() {
            return None;
        }
        let binding = self
            .item_specializations
            .values()
            .next()?
            .substitutions
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter.node.name.node.name.as_str())?;
        select_typed_corelib_value_service(dispatch, binding.managed_reference_kind(), binding.argument)
    }

    /// Managed-reference classification after applying the specialization of the item being
    /// lowered. Generic parameter paths must use their concrete source substitution: their
    /// pointer-shaped ABI alone cannot distinguish a managed nominal/string from a native pointer.
    pub(super) fn managed_reference_in_context(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::PathExpression) {
            let declaration = self.query(resolved_local(self.db, key))?.declaration;
            let slot = self.query(local_slot(self.db, declaration))?;
            if let Some(parameter_name) = self.generic_parameter_name_for_declaration(slot.owner, declaration) {
                let binding = self
                    .item_specializations
                    .get(&slot.owner)?
                    .substitutions
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == parameter_name.as_ref())?;
                return Some(match binding.managed_reference_kind() {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                });
            }
        }
        self.query(managed_reference_kind(self.db, key)).map(|kind| match kind {
            ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
            ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
        })
    }

    fn generic_parameter_name_for_declaration(
        &self,
        key: AstNodeKey,
        declaration: AstNodeKey,
    ) -> Option<std::sync::Arc<str>> {
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::Parameter) {
            let declares_local = self.raw_children(key).into_iter().any(|child| child == declaration);
            return declares_local.then(|| self.query(parameter_generic_reference(self.db, key))).flatten();
        }
        self.raw_children(key)
            .into_iter()
            .find_map(|child| self.generic_parameter_name_for_declaration(child, declaration))
    }

    /// Exact applied field shapes for a struct literal in the item currently being lowered.
    ///
    /// This is the common authority for matching named literal values to physical layout slots;
    /// generic item substitutions are applied before the unspecialized literal fact is considered.
    fn aggregate_literal_layout_in_context(&self, key: AstNodeKey) -> Option<AggregateLayoutFact> {
        self.item_specializations
            .values()
            .next()
            .and_then(|enclosing| {
                self.query(aggregate_literal_specialization(self.db, key, enclosing.substitutions.clone()))
            })
            .or_else(|| self.query(aggregate_literal_layout(self.db, key)))
    }

    pub(super) fn struct_fields_in_layout_order(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::StructLiteralExpression)).then_some(())?;
        let layout = self.aggregate_literal_layout_in_context(key)?;
        let source_fields = self.query(aggregate_literal_field_values(self.db, key))?;
        let mut values_by_name = HashMap::with_capacity(source_fields.len());
        for (name, value) in source_fields.iter() {
            let value = self.unwrap_transparent(*value)?;
            if values_by_name.insert(name.as_ref(), value).is_some() {
                return None;
            }
        }

        let fields =
            layout.fields.iter().map(|(name, _)| values_by_name.remove(name.as_ref())).collect::<Option<Vec<_>>>()?;
        values_by_name.is_empty().then_some(fields)
    }

    pub(super) fn aggregate_field_access_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::AggregateFieldAccess> {
        self.query(aggregate_field_access(self.db, key)).or_else(|| {
            let enclosing = self.item_specializations.values().next()?;
            self.query(aggregate_field_access_specialization(self.db, key, enclosing.substitutions.clone()))
        })
    }

    pub(super) fn array_index_element_type_in_context(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if let Some(element) = self.query(array_index_element_abi_type(self.db, key)) {
            return Some(element);
        }
        let enclosing = self.item_specializations.values().next()?;
        self.query(array_index_element_specialization(self.db, key, enclosing.substitutions.clone()))
    }

    pub(super) fn specialized_enum_constructor(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::EnumConstructorSpecialization> {
        let enclosing = self.item_specializations.values().next()?;
        self.query(enum_constructor_specialization(self.db, key, enclosing.substitutions.clone()))
    }

    pub(super) fn generic_call_specialization_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::GenericSpecializationInstance> {
        if let Some(enclosing) = self.item_specializations.values().next()
            && let Some(specialization) =
                self.query(generic_call_specialization_in_environment(self.db, key, enclosing))
        {
            return Some(specialization);
        }
        self.query(generic_call_specialization(self.db, key))
            .and_then(|specialization| self.query(generic_call_specialization_instance(self.db, specialization)))
    }

    pub(super) fn struct_layout_for_literal(&self, key: AstNodeKey) -> Option<StructLayout> {
        let plan =
            self.input.aggregate_static_plan_for_specialization(key, self.item_specializations.values().next())?;
        self.struct_layout_from_object(plan.object_size, plan.object_alignment, &plan.fields)
    }

    pub(super) fn struct_layout_for_access(
        &self,
        access: &beskid_queries::AggregateFieldAccess,
    ) -> Option<StructLayout> {
        let layout = self.input.aggregate_object_layout_for_access(access)?;
        self.struct_layout_from_object(layout.object_size, layout.object_alignment, &layout.fields)
    }

    /// Translate an ABI-v5 managed object layout into the ISLE struct layout.
    ///
    /// Both the literal (construction) and declaration (field access) paths route through here so a
    /// field is always addressed at the header-relative offset the allocation reserved for it.
    fn struct_layout_from_object(
        &self,
        object_size: u64,
        object_alignment: u64,
        fields: &[AggregateStaticField],
    ) -> Option<StructLayout> {
        let isa = self.isa?;
        let fields = fields
            .iter()
            .map(|field| {
                Some(FieldLayout::new(
                    map_signature_type(isa, field.abi_type)?,
                    u32::try_from(field.field_offset).ok()?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(StructLayout::new(u32::try_from(object_size).ok()?, object_alignment.ilog2() as u8, fields))
    }

    pub(super) fn enum_layout_for(&self, key: AstNodeKey) -> Option<EnumLayout> {
        let source = self
            .query(enum_layout(self.db, key))
            .or_else(|| self.specialized_enum_constructor(key).map(|fact| fact.layout))
            .or_else(|| self.query(enum_match(self.db, key)).map(|fact| fact.layout))?;
        self.enum_layout_from_fact(&source)
    }

    pub(super) fn enum_layout_from_fact(&self, source: &beskid_queries::EnumLayoutFact) -> Option<EnumLayout> {
        let isa = self.isa?;
        let header = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let physical =
            source.scalar_payload_object_layout(self.input.target().pointer_width, header.size, header.alignment)?;
        self.build_enum_layout(isa, physical)
    }

    pub(super) fn match_payload_pattern(
        &self,
        pattern: &beskid_queries::EnumMatchPatternFact,
    ) -> Option<MatchPayloadPatternFact> {
        match pattern {
            beskid_queries::EnumMatchPatternFact::Wildcard => Some(MatchPayloadPatternFact::Ignore),
            beskid_queries::EnumMatchPatternFact::UnitLiteral { .. } => Some(MatchPayloadPatternFact::Unit),
            beskid_queries::EnumMatchPatternFact::Binding(binding) => {
                let slot = self.query(local_slot(self.db, binding.declaration))?;
                let value_type = match binding.payload {
                    AggregateFieldShape::Scalar(semantic) => map_signature_type(self.isa?, semantic)?,
                    AggregateFieldShape::Nominal(_) => self.isa?.pointer_type(),
                };
                let managed_reference = match binding.managed_reference {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                };
                Some(MatchPayloadPatternFact::Binding(MatchArmBindingFact {
                    slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                    value_type,
                    managed_reference,
                }))
            }
            beskid_queries::EnumMatchPatternFact::ScalarLiteral(literal) => {
                Some(MatchPayloadPatternFact::ScalarLiteral {
                    expression: literal.literal,
                    value_type: map_signature_type(self.isa?, literal.semantic_type)?,
                })
            }
            beskid_queries::EnumMatchPatternFact::Enum(pattern) => Some(MatchPayloadPatternFact::Enum {
                layout: self.enum_layout_from_fact(&pattern.layout)?,
                discriminant: u64::from(pattern.variant_index),
                payload: Box::new(MatchPayloadPatternFact::Fields(
                    pattern
                        .items
                        .iter()
                        .map(|payload| self.match_payload_pattern(payload))
                        .collect::<Option<Vec<_>>>()?,
                )),
            }),
        }
    }

    /// Translate the semantic layer's authoritative physical enum records into ISLE layout facts.
    fn build_enum_layout(
        &self,
        isa: &dyn TargetIsa,
        physical: beskid_queries::EnumScalarPayloadObjectLayout,
    ) -> Option<EnumLayout> {
        let tag = FieldLayout::new(types::I32, u32::try_from(physical.tag_offset).ok()?);
        let variants = physical
            .variants
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                let payload_fields = variant
                    .payload_fields
                    .iter()
                    .map(|payload| match *payload {
                        Some((semantic, offset)) => Some(Some(FieldLayout::new(
                            map_signature_type(isa, semantic)?,
                            u32::try_from(offset).ok()?,
                        ))),
                        None => Some(None),
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(EnumVariantLayout::new(u64::try_from(index).ok()?, payload_fields))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(EnumLayout::new(
            u32::try_from(physical.object_size).ok()?,
            physical.object_alignment.ilog2() as u8,
            tag,
            variants,
        ))
    }

    pub(super) fn array_elements_for_literal(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression))
            .then(|| self.raw_children(key).into_iter().filter_map(|child| self.unwrap_transparent(child)).collect())
    }

    pub(super) fn array_layout_for_literal(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.input.array_static_plan(key)?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(beskid_isle::ArrayLayout::new(element, stride, length, plan.alignment.ilog2() as u8))
    }

    /// Layout for a `bulk`-parameter call's packed array, keyed on the `CallExpression` node.
    ///
    /// Mirrors [`array_layout_for_literal`] but reads [`CodegenInput::bulk_array_static_plan`]: the
    /// element ABI comes from the callee's declared `bulk T[]` parameter (declared type over
    /// inferred, the same authority [`array_layout_for_literal`] uses for empty literals), and the
    /// length comes from the call's scalar argument count.
    pub(super) fn array_layout_for_bulk(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.input.bulk_array_static_plan(key)?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(beskid_isle::ArrayLayout::new(element, stride, length, plan.alignment.ilog2() as u8))
    }

    pub(super) fn typed_array_plan(&self, key: AstNodeKey) -> Option<crate::ArrayStaticPlan> {
        self.input.typed_array_static_plan(key, self.item_specializations.values().next())
    }

    pub(super) fn array_layout_for_typed_allocation(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.typed_array_plan(key)?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(beskid_isle::ArrayLayout::new(element, stride, length, plan.alignment.ilog2() as u8))
    }

    /// The bulk calling-convention fact for the callee of one `CallExpression`.
    ///
    /// Resolves the callee declaration via [`call_lowering`], then walks the declaration's
    /// parameter children for the first `bulk` parameter. Returns `None` for non-direct calls
    /// and for direct calls whose callee declares no `bulk` parameter — so it is a safe
    /// classification authority for [`CallKind::Bulk`].
    pub(super) fn callee_bulk_parameter(&self, key: AstNodeKey) -> Option<beskid_queries::BulkParameterFact> {
        let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
            return None;
        };
        let parameters = self.query(child_nodes(self.db, declaration))?;
        for parameter in parameters.iter().copied() {
            if self.query(node_kind(self.db, parameter)) != Some(beskid_queries::IndexedNodeKind::Parameter) {
                continue;
            }
            if let Some(fact) = self.query(bulk_parameter(self.db, parameter)) {
                return Some(fact);
            }
        }
        None
    }

    pub(super) fn runtime_intrinsic(&self, key: AstNodeKey) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.runtime_intrinsic_for(key, &name.0)
    }

    pub(super) fn scheduler_compiler_operation(&self, key: AstNodeKey) -> Option<crate::SchedulerCompilerOperation> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.scheduler_compiler_operation_for(key, &name.0)
    }

    pub(super) fn collect_function_parameters(
        &self,
        key: AstNodeKey,
        parameters: &mut Vec<ParameterSlot>,
    ) -> Option<()> {
        for child in self.raw_children(key) {
            match self.query(node_kind(self.db, child))? {
                beskid_queries::IndexedNodeKind::Block => continue,
                beskid_queries::IndexedNodeKind::Parameter => {
                    let identifier = self.raw_children(child).into_iter().find(|candidate| {
                        self.query(node_kind(self.db, *candidate)) == Some(beskid_queries::IndexedNodeKind::Identifier)
                    })?;
                    let slot = self.query(local_slot(self.db, identifier))?;
                    let specialization = self
                        .item_specializations
                        .get(&key)
                        .and_then(|specialization| specialization.signature.parameters.get(parameters.len()))
                        .copied();
                    let value_type = specialization
                        .or_else(|| {
                            self.query(item_abi_signature(self.db, key))
                                .and_then(|signature| signature.parameters.get(parameters.len()).copied())
                        })
                        .or_else(|| self.scalar_semantic_type(identifier))
                        .and_then(|semantic| {
                            if matches!(
                                semantic,
                                SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING
                            ) {
                                self.isa.map(|isa| isa.pointer_type())
                            } else {
                                map_scalar_type(semantic)
                            }
                        })?;
                    let managed_reference =
                        if let Some(parameter_name) = self.query(parameter_generic_reference(self.db, child)) {
                            let binding = self
                                .item_specializations
                                .get(&key)?
                                .substitutions
                                .iter()
                                .find(|binding| binding.parameter.as_ref() == parameter_name.as_ref())?;
                            match binding.managed_reference_kind() {
                                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                            }
                        } else {
                            self.managed_reference(identifier)?
                        };
                    parameters.push(ParameterSlot {
                        slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                        value_type,
                        managed_reference,
                    });
                }
                _ => self.collect_function_parameters(child, parameters)?,
            }
        }
        Some(())
    }

    pub(super) fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if self.query(implicit_method_receiver(self.db, key)).is_some() {
            return Some(SemanticTypeId::POINTER);
        }
        if self.query(generic_call_template(self.db, key)).is_some() {
            return Some(self.generic_call_specialization_in_context(key)?.signature.result);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(signature) = self.query(call_abi_signature(self.db, key))
        {
            return Some(signature.result);
        }
        match self.query(node_kind(self.db, key)) {
            Some(beskid_queries::IndexedNodeKind::IndexExpression) => {
                return self.array_index_element_type_in_context(key);
            }
            Some(beskid_queries::IndexedNodeKind::AssignExpression) => {
                if let Some(element) = self.array_index_element_type_in_context(key) {
                    return Some(element);
                }
            }
            _ => {}
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::ForStatement) {
            return self.query(for_iterator_fact(self.db, key)).map(|fact| fact.element_type);
        }
        self.specialized_direct_parameter_type(key)
            .or_else(|| {
                self.query(beskid_queries::value_abi_type(self.db, key))
                    .or_else(|| {
                        self.query(cast_intents(self.db, key))
                            .and_then(|intents| intents.first().map(|intent| intent.to))
                    })
                    .or_else(|| {
                        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement))
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
            })
            .or_else(|| {
                self.aggregate_field_access_in_context(key).and_then(|access| {
                    access.layout.fields.get(usize::try_from(access.index).ok()?).map(|(_, shape)| match shape {
                        AggregateFieldShape::Scalar(semantic) => *semantic,
                        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                    })
                })
            })
    }

    pub(super) fn literal(&self, key: AstNodeKey) -> Option<LiteralFact> {
        self.query(literal_fact(self.db, key)).or_else(|| {
            self.query(child_nodes(self.db, key))?.iter().find_map(|child| self.query(literal_fact(self.db, *child)))
        })
    }

    pub(super) fn clif_block_body_for(&self, key: AstNodeKey) -> Option<String> {
        self.query(clif_block_body(self.db, key)).map(|body| body.as_ref().to_string())
    }

    pub(super) fn children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        let children = self.raw_children(key);
        let children = if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::TestDefinition) {
            children
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child)) == Some(beskid_queries::IndexedNodeKind::Statement)
                })
                .collect()
        } else {
            children
        };
        children.into_iter().filter_map(|child| self.unwrap_transparent(child)).collect()
    }

    pub(super) fn raw_children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        self.query(child_nodes(self.db, key)).as_deref().into_iter().flatten().copied().collect()
    }

    pub(super) fn unwrap_transparent(&self, mut key: AstNodeKey) -> Option<AstNodeKey> {
        loop {
            let kind = self.query(node_kind(self.db, key))?;
            // ElseBranch is a structural wrapper around Block or nested If; peel it so
            // emit_if_else can lower the concrete else arm directly.
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

fn select_typed_corelib_value_service(
    dispatch: beskid_abi::runtime_source::CorelibServiceValueDispatch,
    managed: ManagedReferenceKind,
    semantic: SemanticTypeId,
) -> Option<(&'static str, SemanticTypeId)> {
    match (managed, semantic) {
        (ManagedReferenceKind::GcManaged, SemanticTypeId::POINTER) => {
            Some((dispatch.managed_symbol, SemanticTypeId::POINTER))
        }
        (ManagedReferenceKind::NativeOrScalar, semantic) if semantic != SemanticTypeId::POINTER => {
            Some((dispatch.scalar_symbol, semantic))
        }
        _ => None,
    }
}

#[cfg(test)]
mod typed_corelib_value_service_tests {
    use super::*;

    const DISPATCH: beskid_abi::runtime_source::CorelibServiceValueDispatch =
        beskid_abi::runtime_source::CorelibServiceValueDispatch {
            scalar_symbol: "channel_receive_value",
            managed_symbol: "channel_receive_ptr",
        };

    #[test]
    fn scalar_and_managed_values_select_distinct_canonical_adapters() {
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::NativeOrScalar, SemanticTypeId::I64),
            Some(("channel_receive_value", SemanticTypeId::I64))
        );
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::GcManaged, SemanticTypeId::POINTER),
            Some(("channel_receive_ptr", SemanticTypeId::POINTER))
        );
    }

    #[test]
    fn unproven_native_pointer_shape_fails_closed() {
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::NativeOrScalar, SemanticTypeId::POINTER),
            None
        );
    }
}
