//! Array element, literal, bulk, and typed-allocation layout facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn array_index_element_type_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<SemanticTypeId> {
        if let Some(element) = self.query(array_index_element_abi_type(self.db, key)) {
            return Some(element);
        }
        let enclosing = self.current_item_specialization()?;
        self.query(array_index_element_specialization(self.db, key, enclosing.substitutions.clone()))
    }

    pub(in crate::isle_adapter) fn array_elements_for_literal(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression))
            .then(|| self.raw_children(key).into_iter().filter_map(|child| self.unwrap_transparent(child)).collect())
    }

    pub(in crate::isle_adapter) fn array_layout_for_literal(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.input.array_static_plan_for_specialization(key, self.current_item_specialization())?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(scalar_aware_array_layout(plan.element_type, element, stride, length, plan.alignment.ilog2() as u8))
    }

    /// Layout for a `bulk`-parameter call's packed array, keyed on the `CallExpression` node.
    ///
    /// Mirrors [`array_layout_for_literal`] but reads [`CodegenInput::bulk_array_static_plan`]: the
    /// element ABI comes from the callee's declared `bulk T[]` parameter (declared type over
    /// inferred, the same authority [`array_layout_for_literal`] uses for empty literals), and the
    /// length comes from the call's scalar argument count.
    pub(in crate::isle_adapter) fn array_layout_for_bulk(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.input.bulk_array_static_plan(key)?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(scalar_aware_array_layout(plan.element_type, element, stride, length, plan.alignment.ilog2() as u8))
    }

    pub(in crate::isle_adapter) fn typed_array_plan(&self, key: AstNodeKey) -> Option<crate::ArrayStaticPlan> {
        self.input.typed_array_static_plan(key, self.current_item_specialization())
    }

    pub(in crate::isle_adapter) fn array_layout_for_typed_allocation(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_isle::ArrayLayout> {
        let plan = self.typed_array_plan(key)?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(scalar_aware_array_layout(plan.element_type, element, stride, length, plan.alignment.ilog2() as u8))
    }

    /// Whether one `CallExpression` calls a callee ending in a `bulk T[]` parameter, and so packs
    /// its trailing arguments into one array: the classification authority for
    /// [`CallKind::Bulk`].
    pub(in crate::isle_adapter) fn is_bulk_call(&self, key: AstNodeKey) -> bool {
        self.query(bulk_call_element_abi_type(self.db, key)).is_some()
    }
}
