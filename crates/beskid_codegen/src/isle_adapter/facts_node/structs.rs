//! Struct, field, array, and function-parameter layout facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn array_elements_impl(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.array_elements_for_literal(key).or_else(|| self.typed_array_plan(key).map(|_| Vec::new()))
    }

    pub(super) fn array_layout_impl(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        self.array_layout_for_literal(key)
            .or_else(|| self.array_layout_for_bulk(key))
            .or_else(|| self.array_layout_for_typed_allocation(key))
            .or_else(|| {
                let element_type = map_signature_type(self.isa?, self.array_index_element_type_in_context(key)?)?;
                let stride = element_type.bytes();
                Some(beskid_isle::ArrayLayout::new(element_type, stride, 0, stride.ilog2() as u8))
            })
    }

    pub(super) fn managed_array_allocation_impl(&self, key: AstNodeKey) -> Option<beskid_isle::ManagedArrayAllocation> {
        let plan = self
            .input
            .array_static_plan_for_specialization(key, self.current_item_specialization())
            .or_else(|| self.input.bulk_array_static_plan(key))
            .or_else(|| self.typed_array_plan(key))?;
        Some(beskid_isle::ManagedArrayAllocation { allocation_request_symbol: plan.allocation_request_symbol.into() })
    }

    pub(super) fn function_parameters_impl(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        let mut parameters = Vec::new();
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::MethodDefinition) {
            parameters.push(ParameterSlot {
                // Methods cannot spell `self` in Beskid source. The ABI receiver still needs a
                // materialized local so its declared pointer position is consumed by ISLE.
                slot: super::super::context::IMPLICIT_METHOD_RECEIVER_SLOT,
                value_type: self.isa?.pointer_type(),
                managed_reference: ManagedReferenceFact::GcManaged,
            });
        }
        self.collect_function_parameters(key, &mut parameters)?;
        Some(parameters)
    }

    pub(super) fn struct_fields_impl(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.struct_fields_in_layout_order(key)
    }

    pub(super) fn struct_layout_impl(&self, key: AstNodeKey) -> Option<StructLayout> {
        self.struct_layout_for_literal(key).or_else(|| {
            self.aggregate_field_access_in_context(key).and_then(|access| self.struct_layout_for_access(&access))
        })
    }

    pub(super) fn managed_struct_allocation_impl(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        Some(ManagedStructAllocation {
            allocation_request_symbol: self
                .input
                .aggregate_static_plan_for_specialization(key, self.current_item_specialization())
                .or_else(|| self.input.enum_static_plan_for_specialization(key, self.current_item_specialization()))?
                .allocation_request_symbol
                .into(),
        })
    }

    pub(super) fn field_index_impl(&self, key: AstNodeKey) -> Option<u32> {
        self.aggregate_field_access_in_context(key).map(|access| access.index)
    }

    pub(super) fn field_receiver_slot_impl(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        let access = self.aggregate_field_access_in_context(key)?;
        if self.query(node_kind(self.db, access.receiver)) == Some(beskid_queries::IndexedNodeKind::MethodDefinition) {
            return Some(super::super::context::IMPLICIT_METHOD_RECEIVER_SLOT);
        }
        self.query(local_slot(self.db, access.receiver))
            .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
    }
}
