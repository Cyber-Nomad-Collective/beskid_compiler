//! Aggregate literal, field access, and struct layout facts in the lowering context.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    /// Exact applied field shapes for a struct literal in the item currently being lowered.
    ///
    /// This is the common authority for matching named literal values to physical layout slots;
    /// generic item substitutions are applied before the unspecialized literal fact is considered.
    fn aggregate_literal_layout_in_context(&self, key: AstNodeKey) -> Option<AggregateLayoutFact> {
        self.current_item_specialization()
            .and_then(|enclosing| {
                self.query(aggregate_literal_specialization(self.db, key, enclosing.substitutions.clone()))
            })
            .or_else(|| self.query(aggregate_literal_layout(self.db, key)))
    }

    pub(in crate::isle_adapter) fn struct_fields_in_layout_order(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
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

    pub(in crate::isle_adapter) fn aggregate_field_access_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::AggregateFieldAccess> {
        self.query(aggregate_field_access(self.db, key)).or_else(|| {
            let enclosing = self.current_item_specialization()?;
            self.query(aggregate_field_access_specialization(self.db, key, enclosing))
        })
    }

    pub(in crate::isle_adapter) fn struct_layout_for_literal(&self, key: AstNodeKey) -> Option<StructLayout> {
        let plan = self.input.aggregate_static_plan_for_specialization(key, self.current_item_specialization())?;
        self.struct_layout_from_object(plan.object_size, plan.object_alignment, &plan.fields)
    }

    pub(in crate::isle_adapter) fn struct_layout_for_access(
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
}
