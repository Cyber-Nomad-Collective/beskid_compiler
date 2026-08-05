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
        let allocation = self.input.enum_static_plan(key)?;
        let source = if self.query(enum_constructor(self.db, key)).is_some() {
            self.query(enum_layout(self.db, key))?
        } else {
            self.query(enum_match(self.db, key))?.layout
        };
        self.build_enum_layout(isa, allocation, source)
    }

    /// Build an `EnumLayout` from a static plan and source layout fact.
    fn build_enum_layout(
        &self,
        isa: &dyn TargetIsa,
        allocation: AggregateStaticPlan,
        source: beskid_queries::EnumLayoutFact,
    ) -> Option<EnumLayout> {
        let tag_type = types::I32;
        let tag = FieldLayout::new(tag_type, u32::try_from(allocation.fields.first()?.field_offset).ok()?);
        let mut alignment = u32::try_from(allocation.object_alignment).ok()?;
        let payload_offset = allocation.fields.get(1).and_then(|field| u32::try_from(field.field_offset).ok());
        let mut variants = Vec::with_capacity(source.variants.len());
        let mut payloads = Vec::with_capacity(source.variants.len());
        for variant in source.variants.iter() {
            let payload = match variant.fields.as_ref() {
                [] => None,
                [(_, AggregateFieldShape::Scalar(semantic))] => Some(map_signature_type(isa, *semantic)?),
                [(_, AggregateFieldShape::Nominal(_))] => Some(isa.pointer_type()),
                _ => return None,
            };
            if let Some(payload) = payload {
                alignment = alignment.max(payload.bytes());
            }
            payloads.push(payload);
        }
        if payloads.iter().any(Option::is_some) && payload_offset.is_none() {
            return None;
        }
        for (index, payload) in payloads.into_iter().enumerate() {
            variants.push(EnumVariantLayout::new(
                u64::try_from(index).ok()?,
                payload.map(|value_type| FieldLayout::new(value_type, payload_offset.expect("payload offset exists"))),
            ));
        }
        Some(EnumLayout::new(u32::try_from(allocation.object_size).ok()?, alignment.ilog2() as u8, tag, variants))
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

    pub(super) fn runtime_intrinsic(&self, key: AstNodeKey) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.runtime_intrinsic_for(key, &name.0)
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
            .or_else(|| self.query(cast_intents(self.db, key))
            .and_then(|intents| intents.first().map(|intent| intent.to))
            .or_else(|| self.query(binary_operand_abi_type(self.db, key)))
            .or_else(|| self.query(contextual_integer_literal_abi_type(self.db, key)))
            .or_else(|| self.query(abi_type(self.db, key)))
            .or_else(|| self.query(node_type(self.db, key)))
            .or_else(|| {
                (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement)).then(
                    || {
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
                    },
                )?
            }))
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
            .or_else(|| {
                let (_, intrinsic) = self.runtime_intrinsic(key)?;
                semantic_type_for_runtime_intrinsic(intrinsic)
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
