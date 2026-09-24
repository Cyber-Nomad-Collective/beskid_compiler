//! Enum constructor, layout, and match facts in the lowering context.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn specialized_enum_constructor(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::EnumConstructorSpecialization> {
        let enclosing = self.current_item_specialization()?;
        self.query(enum_constructor_specialization(self.db, key, enclosing.substitutions.clone()))
    }

    pub(in crate::isle_adapter) fn enum_layout_for(&self, key: AstNodeKey) -> Option<EnumLayout> {
        let source = self
            .specialized_enum_constructor(key)
            .map(|fact| fact.layout)
            .or_else(|| self.enum_match_in_context(key).map(|fact| fact.layout))
            .or_else(|| self.query(enum_layout(self.db, key)))?;
        self.enum_layout_from_fact(&source)
    }

    pub(in crate::isle_adapter) fn enum_match_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::EnumMatchFact> {
        self.current_item_specialization()
            .and_then(|enclosing| self.query(enum_match_specialization(self.db, key, enclosing.substitutions.clone())))
            .or_else(|| self.query(enum_match(self.db, key)))
    }

    pub(in crate::isle_adapter) fn enum_layout_from_fact(
        &self,
        source: &beskid_queries::EnumLayoutFact,
    ) -> Option<EnumLayout> {
        let isa = self.isa?;
        let header = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let physical =
            source.scalar_payload_object_layout(self.input.target().pointer_width, header.size, header.alignment)?;
        self.build_enum_layout(isa, physical)
    }

    pub(in crate::isle_adapter) fn match_payload_pattern(
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
}
