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
        let plan = self.input.bulk_array_static_plan(key, self.current_item_specialization())?;
        let element = map_signature_type(self.isa?, plan.element_type)?;
        let stride = u32::try_from(plan.stride).ok()?;
        let length = u32::try_from(plan.length).ok()?;
        Some(scalar_aware_array_layout(plan.element_type, element, stride, length, plan.alignment.ilog2() as u8))
    }

    /// The iterator fact of a `ForStatement`, through the enclosing generic specialization when
    /// the iterable's element names one of the enclosing item's generic parameters.
    pub(in crate::isle_adapter) fn for_iterator_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::ForIteratorFact> {
        if let Some(fact) = self.query(for_iterator_fact(self.db, key)) {
            return Some(fact);
        }
        let enclosing = self.current_item_specialization()?;
        self.query(for_iterator_specialization(self.db, key, enclosing))
    }

    /// The array a `ForStatement` iterates: its element layout (the stride and CLIF element type
    /// of a read; the length is a runtime header field) and the GC classification of its elements.
    /// `None` for `range` loops and for iterables without an array source identity.
    pub(in crate::isle_adapter) fn array_for_fact_impl(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayForFact> {
        if self.query(node_kind(self.db, key)) != Some(beskid_queries::IndexedNodeKind::ForStatement) {
            return None;
        }
        let fact = self.for_iterator_in_context(key)?;
        let ForIterableSource::Array { element_managed } = fact.source else { return None };
        let element = map_signature_type(self.isa?, fact.element_type)?;
        let stride = element.bytes();
        let layout = scalar_aware_array_layout(fact.element_type, element, stride, 0, stride.ilog2() as u8);
        let element_managed = match element_managed {
            ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
            ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
        };
        Some(beskid_isle::ArrayForFact { layout, element_managed })
    }

    /// The nominal iterator a `ForStatement` iterates through the iterator protocol: the exact
    /// `Next()` callee (specialized when its owner is generic, or when the iterable's identity is
    /// only proven in the enclosing specialization), its receiver-to-`Option` signature, the
    /// physical `Option<T>` layout and the GC classification of the elements. `None` for `range`
    /// and array loops and for iterables without a proven protocol.
    pub(in crate::isle_adapter) fn iterator_for_fact_impl(&self, key: AstNodeKey) -> Option<beskid_isle::IteratorForFact> {
        if self.query(node_kind(self.db, key)) != Some(beskid_queries::IndexedNodeKind::ForStatement) {
            return None;
        }
        let fact = self.for_iterator_in_context(key)?;
        let ForIterableSource::Iterator { element_managed } = fact.source else { return None };
        let protocol = self.query(for_iterator_protocol(self.db, key, None)).or_else(|| {
            let enclosing = self.current_item_specialization()?;
            self.query(for_iterator_protocol(self.db, key, Some(enclosing)))
        })?;
        if protocol.element_type != fact.element_type || protocol.element_managed != element_managed {
            return None;
        }
        let next = if protocol.next.substitutions.is_empty() && protocol.next.contract_witnesses.is_empty() {
            DirectCallee::item(protocol.next.declaration)
        } else {
            DirectCallee::specialized_item(protocol.next.declaration, specialization_identity(&protocol.next))
        };
        Some(beskid_isle::IteratorForFact {
            next,
            next_signature: signature_for_item(self.isa?, protocol.next.signature.clone())?,
            option_layout: self.enum_layout_from_fact(&protocol.option_layout)?,
            some_variant: protocol.some_variant,
            element_managed: match element_managed {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            },
        })
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

    /// Whether one `CallExpression` uses the bulk calling convention.
    ///
    /// True when the direct callee ends in a `bulk T[]` parameter, whether or not its element ABI
    /// is provable here: [`bulk_call_element_abi_type`] answers "no fact" only for callees without
    /// a bulk parameter, and an error for a bulk callee it cannot prove (an unresolved generic
    /// element). Classifying an unproven bulk call as `Bulk` keeps it on the packing path, which
    /// then fails closed; the `Direct` fallback would pass a single scalar argument where the
    /// callee expects an array handle (`Array.Of<i64>(7)` would dereference address 7).
    pub(in crate::isle_adapter) fn is_bulk_call(&self, key: AstNodeKey) -> bool {
        !matches!(
            bulk_call_element_abi_type(self.db, key, self.current_item_specialization()),
            Ok(None)
        )
    }
}
