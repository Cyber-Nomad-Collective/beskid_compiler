//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use std::collections::HashMap;

use beskid_analysis::syntax::try_decode_string_literal_token;
use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, DirectCallee, EmissionServices, EnumLayout, EnumVariantLayout, FieldLayout,
    FunctionEmissionError, FunctionEmitter, InlineCaptureField, InlineClosureEnvironment, InlineLambdaCall,
    ItemStatementEmission, LiteralKind, LocalSlotId, ManagedStructAllocation, MatchArmBindingFact, MatchArmFact,
    NodeFacts, NodeKind, OperatorFact, ParameterSlot, RuntimeArrayLayout, RuntimeIntrinsicKind, Signature,
    StringInterner, StructLayout,
};
use beskid_queries::{
    AggregateFieldShape, CallLowering, Db, GenericCallSpecialization, ItemSignature, LiteralFact, SemanticTypeId,
    abi_type, aggregate_field_access, aggregate_layout, aggregate_literal_declaration, block_statement_nodes,
    call_abi_signature, call_argument_abi_type, call_arguments, call_lowering, cast_intents, child_nodes,
    closure_call_target, closure_environment, constant_integer, dispatch_builtin_symbol, enum_constructor,
    enum_constructor_in_item, enum_layout, enum_match, explicit_generic_call_specialization, for_iterator_fact,
    generic_call_specialization, generic_call_specialization_in_item, item_abi_signature, item_body,
    let_initializer as semantic_let_initializer, literal_fact, local_slot, mutable_local_assignment, node_kind,
    node_type, nominal_member_receiver, operator_fact, range_for_fact, resolved_item, resolved_local,
    runtime_intrinsic_name, scalar_match, spawn_entry_validation, specialized_local_abi_type, test_statement_nodes,
    typed_let_call_result,
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
mod mappings;

pub use context::SyntaxNodeFacts;
pub use emit::{
    emit_isle_closure_lambda_entry, emit_isle_expression, emit_isle_expression_with_call_importer, emit_isle_item,
    emit_isle_item_with_call_importer, emit_isle_item_with_services, emit_isle_item_with_services_specialization,
    syntax_item_signature,
};
pub use importer::ItemModuleImporter;
use mappings::*;
