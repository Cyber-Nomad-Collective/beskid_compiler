//! The `NodeFacts` trait consumed by leaf ISLE rules.

use super::*;
use crate::layout::{ArrayLayout, EnumLayout, ManagedArrayAllocation, ManagedStructAllocation, StructLayout};
use cranelift_codegen::ir::{Signature, Type};
use std::sync::Arc;

pub trait NodeFacts {
    fn scoped_cleanup(&self, _key: AstNodeKey) -> Option<ScopedCleanupPlan> {
        None
    }
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind>;
    fn literal_kind(&self, _key: AstNodeKey) -> Option<LiteralKind> {
        None
    }
    fn operator_fact(&self, _key: AstNodeKey) -> Option<OperatorFact> {
        None
    }
    fn call_kind(&self, _key: AstNodeKey) -> Option<CallKind> {
        None
    }
    fn primitive_numeric_conversion(
        &self,
        _key: AstNodeKey,
    ) -> Option<(beskid_queries::SemanticTypeId, beskid_queries::SemanticTypeId)> {
        None
    }
    /// Exact semantic type used to validate a primitive conversion fact before it reaches CLIF.
    fn semantic_type(&self, _key: AstNodeKey) -> Option<beskid_queries::SemanticTypeId> {
        None
    }
    fn managed_reference(&self, _key: AstNodeKey) -> Option<ManagedReferenceFact> {
        None
    }
    /// Syntax/Salsa-proven Result propagation facts for postfix `value?`.
    ///
    /// Implementations must return `None` for stale, foreign, unsupported, or otherwise
    /// unproven nodes so generated ISLE fails closed before CLIF.
    fn try_expression_fact(&self, _key: AstNodeKey) -> Option<beskid_queries::TryExpressionFact> {
        None
    }

    fn try_return_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    fn runtime_intrinsic_kind(&self, _key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        None
    }
    fn collection_operation(&self, _key: AstNodeKey) -> Option<CollectionOperation> {
        None
    }
    fn collection_element_type(&self, _key: AstNodeKey) -> Option<Type> {
        None
    }
    fn child(&self, _key: AstNodeKey, _index: u8) -> Option<AstNodeKey> {
        None
    }
    fn statement_count(&self, _key: AstNodeKey) -> Option<u8> {
        None
    }
    fn block_result(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn let_initializer(&self, _key: AstNodeKey) -> Option<AstNodeKey> {
        None
    }
    fn integer_literal(&self, key: AstNodeKey) -> Option<i64>;
    /// Constant values are immediate and therefore have no local storage slot.
    fn constant_integer(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }
    /// Compiler-minted canonical-runtime constants may be materialized at an
    /// otherwise exact direct-call ABI argument type. Ordinary source never
    /// receives this authority.
    fn canonical_runtime_constant_integer(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }
    fn boolean_literal(&self, _key: AstNodeKey) -> Option<bool> {
        None
    }
    fn float_literal(&self, _key: AstNodeKey) -> Option<f64> {
        None
    }
    fn char_literal(&self, _key: AstNodeKey) -> Option<char> {
        None
    }
    fn string_literal(&self, _key: AstNodeKey) -> Option<Arc<str>> {
        None
    }
    fn scalar_type(&self, key: AstNodeKey) -> Option<Type>;
    fn direct_callee(&self, _key: AstNodeKey) -> Option<DirectCallee> {
        None
    }
    fn call_signature(&self, _key: AstNodeKey) -> Option<Signature> {
        None
    }
    fn call_arguments(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn inline_lambda_call(&self, _key: AstNodeKey) -> Option<InlineLambdaCall> {
        None
    }
    fn array_elements(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn array_layout(&self, _key: AstNodeKey) -> Option<ArrayLayout> {
        None
    }
    fn managed_array_allocation(&self, _key: AstNodeKey) -> Option<ManagedArrayAllocation> {
        None
    }
    fn struct_fields(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn struct_layout(&self, _key: AstNodeKey) -> Option<StructLayout> {
        None
    }
    fn managed_struct_allocation(&self, _key: AstNodeKey) -> Option<ManagedStructAllocation> {
        None
    }
    fn field_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn field_receiver_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    fn enum_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    /// Enum layout suitable for binary comparison: resolves the common enum type of
    /// both operands so discriminant comparison can load the correct tag at the correct
    /// offset. Returns None when either operand is not an enum value.
    fn binary_enum_layout(&self, _key: AstNodeKey) -> Option<EnumLayout> {
        None
    }
    fn enum_variant_index(&self, _key: AstNodeKey) -> Option<u32> {
        None
    }
    fn enum_payloads(&self, _key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        None
    }
    fn match_arms(&self, _key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        None
    }
    fn range_fact(&self, _key: AstNodeKey) -> Option<RangeFact> {
        None
    }
    fn spawn_entry(&self, _key: AstNodeKey) -> Option<SpawnEntry> {
        None
    }
    fn traced_fiber_join_layout(&self, _key: AstNodeKey) -> Option<TracedFiberJoinLayout> {
        None
    }

    fn traced_channel_send_layout(&self, _key: AstNodeKey) -> Option<TracedFiberJoinLayout> {
        None
    }
    fn lambda_entry(&self, _key: AstNodeKey) -> Option<LambdaEntry> {
        None
    }
    fn local_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    /// Proven mutable destination slot for one simple local assignment expression.
    fn mutable_local_assignment_slot(&self, _key: AstNodeKey) -> Option<LocalSlotId> {
        None
    }
    fn dispatch_builtin_symbol(&self, _key: AstNodeKey) -> Option<&'static str> {
        None
    }
    fn index_target_is_string(&self, _key: AstNodeKey) -> bool {
        false
    }
    /// Parameter slots in source order for one function item.
    fn function_parameters(&self, _key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        None
    }
    /// Raw body text of a clif block expression.
    fn clif_block_body(&self, _key: AstNodeKey) -> Option<String> {
        None
    }
}
