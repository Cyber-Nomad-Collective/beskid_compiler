//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use std::collections::HashMap;

use beskid_analysis::syntax::try_decode_string_literal_token;
use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, CollectionMutationOwner, CollectionOperation, DirectCallee, EmissionServices,
    EnumLayout, EnumVariantLayout, FieldLayout, FunctionEmissionError, FunctionEmitter, InlineCaptureField,
    InlineClosureEnvironment, InlineLambdaCall, ItemStatementEmission, LiteralKind, LocalSlotId, ManagedReferenceFact,
    ManagedStructAllocation, MatchArmBindingFact, MatchArmFact, MatchPayloadPatternFact, NodeFacts, NodeKind,
    OperatorFact, ParameterSlot, RuntimeIntrinsicKind, Signature, StringInterner, StructLayout,
};
use beskid_queries::{
    AggregateFieldShape, AggregateLayoutFact, CallLowering, Db, ItemSignature, LiteralFact, ManagedReferenceKind,
    SemanticTypeId, abi_type, aggregate_field_access, aggregate_field_access_specialization,
    aggregate_literal_declaration, aggregate_literal_field_values, aggregate_literal_layout,
    aggregate_literal_specialization, array_index_element_abi_type, array_index_element_specialization,
    block_statement_nodes, bulk_parameter, call_abi_signature, call_argument_abi_type, call_arguments, call_lowering,
    cast_intents, child_nodes, clif_block_body, closure_call_target, closure_environment, closure_signature,
    constant_integer, contextual_integer_literal_abi_type, dispatch_builtin_symbol, enum_constructor,
    enum_constructor_specialization, enum_layout, enum_match, enum_match_specialization, for_iterator_fact,
    generic_call_specialization, generic_call_specialization_in_environment, generic_call_specialization_instance,
    generic_call_template, implicit_method_receiver, item_abi_signature, item_body, literal_fact, local_slot,
    managed_reference_kind, mutable_local_assignment, node_kind, node_type, nominal_member_receiver, operator_fact,
    parameter_generic_reference, range_for_fact, resolved_item, resolved_local, runtime_intrinsic_name,
    spawn_entry_validation, test_statement_nodes, try_expression_fact,
};
use cranelift_codegen::ir::{FuncRef, Type, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};

use crate::{AggregateStaticField, CodegenInput};

mod context;
mod emit;
mod facts_helpers;
mod facts_node;
mod importer;
pub(crate) mod mappings;

pub use context::SyntaxNodeFacts;
pub use emit::{
    emit_isle_closure_lambda_entry, emit_isle_expression, emit_isle_expression_with_call_importer, emit_isle_item,
    emit_isle_item_with_call_importer, emit_isle_item_with_services, emit_isle_item_with_services_specialization,
    syntax_item_signature,
};
pub use importer::ItemModuleImporter;
use mappings::*;
