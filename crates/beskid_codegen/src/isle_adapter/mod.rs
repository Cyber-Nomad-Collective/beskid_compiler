//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use std::collections::HashMap;

use beskid_analysis::syntax::try_decode_string_literal_token;
use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, DirectCallee, EmissionServices, EnumLayout,
    EnumVariantLayout, FieldLayout, FunctionEmissionError, FunctionEmitter, InlineCaptureField,
    InlineClosureEnvironment, InlineLambdaCall, ItemStatementEmission, LiteralKind, LocalSlotId,
    ManagedStructAllocation, MatchArmBindingFact, MatchArmFact, NodeFacts, NodeKind, OperatorFact,
    ParameterSlot, RuntimeIntrinsicKind, Signature, StringInterner, StructLayout,
};
use beskid_queries::{
    AggregateFieldShape, CallLowering, Db, ItemSignature, LiteralFact, SemanticTypeId, abi_type,
    aggregate_field_access, aggregate_layout, aggregate_literal_declaration, block_statement_nodes,
    call_abi_signature, call_argument_abi_type, call_arguments, call_lowering, cast_intents,
    child_nodes, closure_call_target, closure_environment, dispatch_builtin_symbol,
    enum_constructor, enum_layout, enum_match, for_iterator_fact, generic_call_specialization,
    item_abi_signature, item_body, literal_fact, local_slot, mutable_local_assignment, node_kind,
    node_type, nominal_member_receiver, operator_fact, range_for_fact, resolved_item,
    resolved_local, runtime_intrinsic_name, spawn_entry_validation, test_statement_nodes,
};
use cranelift_codegen::ir::{FuncRef, Type, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};

use crate::CodegenInput;

mod context;
mod emit;
mod facts_helpers;
mod facts_node;
mod importer;
mod mappings;

pub use emit::{
    emit_isle_closure_lambda_entry, emit_isle_expression, emit_isle_expression_with_call_importer,
    emit_isle_item, emit_isle_item_with_call_importer, emit_isle_item_with_services,
    emit_isle_item_with_services_specialization, syntax_item_signature,
};
pub use importer::ItemModuleImporter;
pub use context::SyntaxNodeFacts;
use mappings::*;
