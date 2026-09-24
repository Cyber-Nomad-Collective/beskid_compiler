//! Typed Corelib value-service dispatch and managed-reference classification in context.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    /// Resolve one explicitly generic Corelib value service through the immutable specialization
    /// of the item being lowered. Pointer-shaped native values are deliberately rejected because
    /// only source identity can prove that the managed adapter is safe.
    pub(in crate::isle_adapter) fn typed_corelib_value_service(
        &self,
        key: AstNodeKey,
        service: beskid_abi::runtime_source::CorelibService,
    ) -> Option<(&'static str, SemanticTypeId)> {
        let dispatch = beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service)?;
        let binding = self.query(beskid_queries::specialized_corelib_value_service_result(
            self.db,
            key,
            self.current_item_specialization()?,
        ))?;
        select_typed_corelib_value_service(dispatch, binding.managed_reference_kind(), binding.argument)
    }

    /// Managed-reference classification after applying the specialization of the item being
    /// lowered. Generic parameter paths must use their concrete source substitution: their
    /// pointer-shaped ABI alone cannot distinguish a managed nominal/string from a native pointer.
    pub(in crate::isle_adapter) fn managed_reference_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<ManagedReferenceFact> {
        if let Some(receiver) = self.query(nominal_member_receiver(self.db, key)) {
            return self.managed_reference_in_context(receiver);
        }
        if self.typed_array_plan(key).is_some() || self.query(implicit_method_receiver(self.db, key)).is_some() {
            return Some(ManagedReferenceFact::GcManaged);
        }
        if let Some(instance) = self.generic_call_specialization_in_context(key) {
            return self
                .query(beskid_queries::specialized_call_result_managed_reference_kind(self.db, key, &instance))
                .map(|kind| match kind {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                });
        }
        // These compiler-authorized intrinsics produce native addresses. Their
        // pointer-shaped ABI alone cannot prove that category to the source query.
        if matches!(
            self.runtime_intrinsic_kind(key),
            Some(
                RuntimeIntrinsicKind::PointerFromNativeWord
                    | RuntimeIntrinsicKind::PointerAdd
                    | RuntimeIntrinsicKind::SchedulerFiberEntryAddress
                    | RuntimeIntrinsicKind::SchedulerReturnTrampolineAddress
            )
        ) {
            return Some(ManagedReferenceFact::NativeOrScalar);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::PathExpression)
            && let Some(managed) = self.specialized_local_managed_reference(key)
        {
            return Some(managed);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::PathExpression)
            && let Some(binding) = self.specialized_pattern_binding(key)
        {
            return Some(match binding.managed_reference {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            });
        }
        if let Some(kind) = self.query(managed_reference_kind(self.db, key)) {
            return Some(match kind {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            });
        }
        if let Some(operator) = self.operator_fact(key) {
            return Some(if operator == OperatorFact::StringAdd {
                ManagedReferenceFact::GcManaged
            } else {
                ManagedReferenceFact::NativeOrScalar
            });
        }
        // Contextual scalar ABI facts also cover expressions such as native-word
        // arithmetic that have no independently inferred node_type. Only a distinct
        // scalar/string identity is sufficient here; POINTER remains ambiguous.
        if let Some(semantic) =
            self.scalar_semantic_type(key).or_else(|| self.query(call_argument_abi_type(self.db, key)))
            && semantic != SemanticTypeId::POINTER
        {
            return Some(if semantic == SemanticTypeId::STRING {
                ManagedReferenceFact::GcManaged
            } else {
                ManagedReferenceFact::NativeOrScalar
            });
        }
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement))
            .then(|| self.let_initializer(key))?
            .and_then(|initializer| self.managed_reference_in_context(initializer))
    }

    fn specialized_local_managed_reference(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        let declaration = self.query(resolved_local(self.db, key))?.declaration;
        let slot = self.query(local_slot(self.db, declaration))?;
        let parameter_name = self.generic_parameter_name_for_declaration(slot.owner, declaration)?;
        let binding = self
            .item_specializations
            .get(&slot.owner)?
            .substitutions
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter_name.as_ref())?;
        Some(match binding.managed_reference_kind() {
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
}

fn select_typed_corelib_value_service(
    dispatch: beskid_abi::runtime_source::CorelibServiceValueDispatch,
    managed: ManagedReferenceKind,
    semantic: SemanticTypeId,
) -> Option<(&'static str, SemanticTypeId)> {
    match (managed, semantic) {
        (ManagedReferenceKind::GcManaged, SemanticTypeId::POINTER) => Some((dispatch.symbol, SemanticTypeId::POINTER)),
        (ManagedReferenceKind::NativeOrScalar, semantic) if semantic != SemanticTypeId::POINTER => {
            Some((dispatch.symbol, semantic))
        }
        _ => None,
    }
}

#[cfg(test)]
mod typed_corelib_value_service_tests {
    use super::*;

    const DISPATCH: beskid_abi::runtime_source::CorelibServiceValueDispatch =
        beskid_abi::runtime_source::CorelibServiceValueDispatch { symbol: "channel_receive_value" };

    #[test]
    fn scalar_and_managed_values_select_one_canonical_slot_adapter() {
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::NativeOrScalar, SemanticTypeId::I64),
            Some(("channel_receive_value", SemanticTypeId::I64))
        );
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::GcManaged, SemanticTypeId::POINTER),
            Some(("channel_receive_value", SemanticTypeId::POINTER))
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
