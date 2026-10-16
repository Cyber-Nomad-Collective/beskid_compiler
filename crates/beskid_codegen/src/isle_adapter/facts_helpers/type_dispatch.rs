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
        // A typed Corelib value service (`__isolate_join_value<T>`, `__port_receive_value<T>`, ...)
        // returns its `T` through a pointer-shaped transport whose source declaration carries no
        // category; the specialization's binding of `T` classifies the value, so a local it
        // initializes is rooted when `T` is managed.
        if let Some(enclosing) = self.current_item_specialization()
            && let Some(beskid_queries::CallLowering::CorelibService(service)) =
                self.query(beskid_queries::call_lowering(self.db, key))
            && beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service).is_some()
            && let Some(binding) =
                self.query(beskid_queries::specialized_corelib_value_service_result(self.db, key, enclosing))
        {
            return Some(match binding.managed_reference_kind() {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            });
        }
        if let Some(instance) = self.generic_call_specialization_in_context(key) {
            return self
                .query(beskid_queries::specialized_call_result_managed_reference_kind(self.db, key, &instance))
                .map(|kind| match kind {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                });
        }
        // `operand?` in a specialized body is classified by its payload in the instance: a `T`
        // payload bound to a record is managed, bound to `i64` is not.
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::TryExpression)
            && let Some(enclosing) = self.current_item_specialization()
            && let Some(fact) = self.query(beskid_queries::specialized_try_expression_fact(self.db, key, enclosing))
        {
            return Some(match fact.payload_managed_reference_kind() {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            });
        }
        // A call through a function value in a specialized body is classified by its result in
        // the instance: a `T` result bound to `string` is managed, bound to `i64` is not.
        if self.current_item_specialization().is_some()
            && self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(call) = self.function_value_call_in_context(key)
        {
            return Some(match call.result_managed_reference {
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
        // An indexed element typed by an enclosing generic parameter (`values[index]` in
        // `Copy<T>`) is classified by the specialization's source binding, never by its
        // pointer-shaped element ABI.
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::IndexExpression)
            && let Some(enclosing) = self.current_item_specialization()
            && let Some(kind) = self.query(array_index_element_managed_reference_specialization(
                self.db,
                key,
                enclosing,
            ))
        {
            return Some(match kind {
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
        // A field typed by an enclosing generic parameter (`this.binding` with `B binding` in
        // `impl<T, B>`, `decoder.binding` on a `Dec<T, B>` parameter) has no source-level managed
        // category of its own; the specialization's applied field layout carries it.
        if let Some(enclosing) = self.current_item_specialization()
            && let Some(access) = self.query(aggregate_field_access_specialization(self.db, key, enclosing))
            && let Some((_, shape)) = access.layout.fields.get(usize::try_from(access.index).ok()?)
        {
            return Some(match shape {
                AggregateFieldShape::Scalar(semantic) if *semantic == SemanticTypeId::STRING => {
                    ManagedReferenceFact::GcManaged
                }
                AggregateFieldShape::Scalar(_) => ManagedReferenceFact::NativeOrScalar,
                AggregateFieldShape::ManagedReference(_) | AggregateFieldShape::Nominal(_) => {
                    ManagedReferenceFact::GcManaged
                }
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
        // arithmetic that have no independently inferred node_type, and bare integer
        // literals typed by their expected type. Only a distinct scalar/string identity is
        // sufficient here; POINTER remains ambiguous.
        if let Some(semantic) = self
            .scalar_semantic_type(key)
            .or_else(|| self.query(contextual_integer_literal_abi_type(self.db, key)))
            && semantic != SemanticTypeId::POINTER
        {
            return Some(if semantic == SemanticTypeId::STRING {
                ManagedReferenceFact::GcManaged
            } else {
                ManagedReferenceFact::NativeOrScalar
            });
        }
        // A match or block value (`field: match slot { Some(value) => value, None() => ... }`) is
        // classified by the type its position expects, read in the specialization's environment.
        if matches!(
            self.query(node_kind(self.db, key)),
            Some(beskid_queries::IndexedNodeKind::MatchExpression | beskid_queries::IndexedNodeKind::BlockExpression)
        ) && let Some(expected) = match self.current_item_specialization() {
            Some(enclosing) => self.query(beskid_queries::expected_type_in_environment(self.db, key, enclosing)),
            None => self.query(beskid_queries::expected_type(self.db, key)),
        } {
            return Some(match expected.managed_reference_kind() {
                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
            });
        }
        // A match value without an expected type (`let x = match h.slot { ... }` in a
        // specialization) is classified like the arms that produce its value; they must agree.
        if let Some(bodies) = self.specialized_match_arm_bodies(key) {
            let mut result = None;
            for body in bodies {
                if self.scalar_semantic_type(body) == Some(SemanticTypeId::NEVER) {
                    continue;
                }
                let managed = self.managed_reference_in_context(body)?;
                if result.is_some_and(|previous| previous != managed) {
                    return None;
                }
                result = Some(managed);
            }
            return result;
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
        // A managed value crosses as its reference: a nominal, array or function value
        // (`POINTER`) and a `string` (`STRING`) alike.
        (ManagedReferenceKind::GcManaged, SemanticTypeId::POINTER | SemanticTypeId::STRING) => {
            Some((dispatch.symbol, semantic))
        }
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
        assert_eq!(
            select_typed_corelib_value_service(DISPATCH, ManagedReferenceKind::GcManaged, SemanticTypeId::STRING),
            Some(("channel_receive_value", SemanticTypeId::STRING))
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
