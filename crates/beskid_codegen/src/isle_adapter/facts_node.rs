//! `NodeFacts` for `SyntaxNodeFacts`: one delegator per fact, implemented by concern in submodules.

use super::*;

mod calls;
mod collections;
mod enums;
mod literals;
mod shape;
mod structs;
mod types;

impl NodeFacts for SyntaxNodeFacts<'_> {
    fn scoped_cleanup(&self, key: AstNodeKey) -> Option<beskid_isle::ScopedCleanupPlan> {
        self.scoped_cleanup_impl(key)
    }

    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        self.node_kind_impl(key)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.literal_kind_impl(key)
    }

    fn constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        self.constant_integer_impl(key)
    }

    fn canonical_runtime_constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        self.canonical_runtime_constant_integer_impl(key)
    }

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        self.operator_fact_impl(key)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        self.child_impl(key, index)
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        self.statement_count_impl(key)
    }

    fn block_result(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        self.block_result_impl(key)
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        self.let_initializer_impl(key)
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.local_slot_impl(key)
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.mutable_local_assignment_slot_impl(key)
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        self.call_kind_impl(key)
    }

    fn primitive_numeric_conversion(&self, key: AstNodeKey) -> Option<(SemanticTypeId, SemanticTypeId)> {
        self.primitive_numeric_conversion_impl(key)
    }

    fn semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.semantic_type_impl(key)
    }

    fn managed_reference(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        self.managed_reference_impl(key)
    }

    fn try_expression_fact(&self, key: AstNodeKey) -> Option<beskid_queries::TryExpressionFact> {
        self.try_expression_fact_impl(key)
    }

    fn try_return_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.try_return_layout_impl(key)
    }

    fn index_target_is_string(&self, key: AstNodeKey) -> bool {
        self.index_target_is_string_impl(key)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        self.runtime_intrinsic_kind_impl(key)
    }

    fn collection_operation(&self, key: AstNodeKey) -> Option<CollectionOperation> {
        self.collection_operation_impl(key)
    }

    fn collection_element_type(&self, key: AstNodeKey) -> Option<Type> {
        self.collection_element_type_impl(key)
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        self.direct_callee_impl(key)
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        self.call_signature_impl(key)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.call_arguments_impl(key)
    }

    fn inline_lambda_call(&self, key: AstNodeKey) -> Option<InlineLambdaCall> {
        self.inline_lambda_call_impl(key)
    }

    fn array_elements(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.array_elements_impl(key)
    }

    fn array_layout(&self, key: AstNodeKey) -> Option<beskid_isle::ArrayLayout> {
        self.array_layout_impl(key)
    }

    fn managed_array_allocation(&self, key: AstNodeKey) -> Option<beskid_isle::ManagedArrayAllocation> {
        self.managed_array_allocation_impl(key)
    }

    fn function_parameters(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        self.function_parameters_impl(key)
    }

    fn clif_block_body(&self, key: AstNodeKey) -> Option<String> {
        self.clif_block_body_impl(key)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        self.integer_literal_impl(key)
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        self.boolean_literal_impl(key)
    }

    fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
        self.float_literal_impl(key)
    }

    fn char_literal(&self, key: AstNodeKey) -> Option<char> {
        self.char_literal_impl(key)
    }

    fn string_literal(&self, key: AstNodeKey) -> Option<std::sync::Arc<str>> {
        self.string_literal_impl(key)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        self.scalar_type_impl(key)
    }

    fn struct_fields(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.struct_fields_impl(key)
    }

    fn struct_layout(&self, key: AstNodeKey) -> Option<StructLayout> {
        self.struct_layout_impl(key)
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        self.managed_struct_allocation_impl(key)
    }

    fn field_index(&self, key: AstNodeKey) -> Option<u32> {
        self.field_index_impl(key)
    }

    fn field_receiver_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.field_receiver_slot_impl(key)
    }

    fn enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.enum_layout_impl(key)
    }

    fn binary_enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.binary_enum_layout_impl(key)
    }

    fn enum_variant_index(&self, key: AstNodeKey) -> Option<u32> {
        self.enum_variant_index_impl(key)
    }

    fn enum_payloads(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.enum_payloads_impl(key)
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        self.match_arms_impl(key)
    }

    fn range_fact(&self, key: AstNodeKey) -> Option<beskid_isle::RangeFact> {
        self.range_fact_impl(key)
    }

    fn spawn_entry(&self, key: AstNodeKey) -> Option<beskid_isle::SpawnEntry> {
        self.spawn_entry_impl(key)
    }

    fn traced_fiber_join_layout(&self, key: AstNodeKey) -> Option<beskid_isle::TracedFiberJoinLayout> {
        self.traced_fiber_join_layout_impl(key)
    }

    fn traced_channel_send_layout(&self, key: AstNodeKey) -> Option<beskid_isle::TracedFiberJoinLayout> {
        self.traced_channel_send_layout_impl(key)
    }

    fn lambda_entry(&self, key: AstNodeKey) -> Option<beskid_isle::LambdaEntry> {
        self.lambda_entry_impl(key)
    }
}
