use super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn struct_layout_for_literal(&self, key: AstNodeKey) -> Option<StructLayout> {
        let plan = self.input.aggregate_static_plan(key)?;
        self.struct_layout_from_object(plan.object_size, plan.object_alignment, &plan.fields)
    }

    pub(super) fn struct_layout_for_declaration(&self, declaration: AstNodeKey) -> Option<StructLayout> {
        let layout = self.input.aggregate_object_layout(declaration)?;
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
        let isa = self.isa?;
        let source = self
            .query(enum_layout(self.db, key))
            .or_else(|| self.query(enum_match(self.db, key)).map(|fact| fact.layout))?;
        let header = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let physical =
            source.scalar_payload_object_layout(self.input.target().pointer_width, header.size, header.alignment)?;
        self.build_enum_layout(isa, physical)
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
                let payload = match (variant.payload_type, variant.payload_offset) {
                    (None, None) => None,
                    (Some(semantic), Some(offset)) => {
                        Some(FieldLayout::new(map_signature_type(isa, semantic)?, u32::try_from(offset).ok()?))
                    }
                    _ => return None,
                };
                Some(EnumVariantLayout::new(u64::try_from(index).ok()?, payload))
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
                    parameters.push(ParameterSlot {
                        slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                        value_type,
                    });
                }
                _ => self.collect_function_parameters(child, parameters)?,
            }
        }
        Some(())
    }

    pub(super) fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
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
                self.query(aggregate_field_access(self.db, key)).and_then(|access| {
                    self.query(aggregate_layout(self.db, access.declaration))?
                        .fields
                        .get(usize::try_from(access.index).ok()?)
                        .map(|(_, shape)| match shape {
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
