use super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn struct_layout_for_literal(&self, key: AstNodeKey) -> Option<StructLayout> {
        let plan = self.input.aggregate_static_plan(key)?;
        let fields = plan
            .fields
            .iter()
            .map(|field| {
                Some(FieldLayout::new(
                    map_signature_type(self.isa?, field.abi_type)?,
                    u32::try_from(field.field_offset).ok()?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(StructLayout::new(u32::try_from(plan.object_size).ok()?, plan.object_alignment.ilog2() as u8, fields))
    }

    pub(super) fn struct_layout_for_declaration(&self, declaration: AstNodeKey) -> Option<StructLayout> {
        let isa = self.isa?;
        let aggregate = self.query(aggregate_layout(self.db, declaration))?;
        let header = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let mut size = u32::try_from(header.size).ok()?;
        let mut alignment = u32::try_from(header.alignment).ok()?;
        (size >= 16 && alignment.is_power_of_two() && alignment > 0).then_some(())?;
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
        let size = align_to(size, alignment)?.max(1);
        Some(StructLayout::new(size, alignment.ilog2() as u8, fields))
    }

    pub(super) fn enum_layout_for(&self, key: AstNodeKey) -> Option<EnumLayout> {
        let isa = self.isa?;
        let contextual = self.item_specializations.iter().find_map(|(item, specialization)| {
            self.query(enum_constructor_in_item(self.db, key, *item, specialization))
                .map(|(_, layout)| (item, specialization, layout))
        });
        let allocation = self.input.enum_static_plan(key).or_else(|| {
            contextual
                .as_ref()
                .and_then(|(item, specialization, _)| self.input.enum_static_plan_in_item(key, **item, specialization))
        })?;
        let source = if let Some((_, _, layout)) = contextual {
            layout
        } else if self.query(enum_constructor(self.db, key)).is_some() {
            self.query(enum_layout(self.db, key))?
        } else {
            self.query(enum_match(self.db, key))?.layout
        };
        let tag_type = types::I32;
        let tag_offset = u32::try_from(allocation.fields.first()?.field_offset).ok()?;
        let tag = FieldLayout::new(tag_type, tag_offset);
        let mut alignment = u32::try_from(allocation.object_alignment).ok()?;
        let mut variants = Vec::with_capacity(source.variants.len());
        let payload_start = tag_offset.checked_add(tag_type.bytes())?;
        for (index, variant) in source.variants.iter().enumerate() {
            let mut cursor = payload_start;
            let mut fields = Vec::with_capacity(variant.fields.len());
            for (_, shape) in variant.fields.iter() {
                let value_type = match shape {
                    AggregateFieldShape::Scalar(semantic) => map_signature_type(isa, *semantic)?,
                    AggregateFieldShape::Nominal(_) => isa.pointer_type(),
                };
                let field_alignment = value_type.bytes().next_power_of_two();
                alignment = alignment.max(field_alignment);
                cursor = align_to(cursor, field_alignment)?;
                fields.push(FieldLayout::new(value_type, cursor));
                cursor = cursor.checked_add(value_type.bytes())?;
            }
            (u64::from(cursor) <= allocation.object_size).then_some(())?;
            variants.push(EnumVariantLayout::with_fields(u64::try_from(index).ok()?, fields));
        }
        Some(EnumLayout::new(u32::try_from(allocation.object_size).ok()?, alignment.ilog2() as u8, tag, variants))
    }

    pub(super) fn array_elements_for_literal(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression))
            .then(|| self.raw_children(key).into_iter().filter_map(|child| self.unwrap_transparent(child)).collect())
    }

    pub(super) fn array_layout_for_literal(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let elements = self.array_elements_for_literal(key)?;
        if !elements.is_empty() {
            return None;
        }
        let pointer = self.isa?.pointer_type();
        Some(beskid_isle::ArrayLayout::new(pointer, pointer.bytes(), 0, pointer.bytes().ilog2() as u8))
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
        self.query(cast_intents(self.db, key))
            .and_then(|intents| intents.first().map(|intent| intent.to))
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
