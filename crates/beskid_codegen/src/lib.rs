//! Cranelift-based code generation for Beskid.

#![allow(clippy::drop_non_drop, clippy::question_mark, clippy::result_large_err)]
//!
//! Pipeline: [`lower_prepared_syntax_entrypoint`] reports the generated syntax-codegen boundary
//! before `codegen_clif`. Mod orchestration ids (`mod.load` … `mod.rewrite`,
//! [`beskid_pipeline::phases::SYNTAX_GENERATION`]) are **not** emitted here; they are reserved for
//! hosts that run the mod SDK and should call [`beskid_pipeline::observe_phase`] around real mod
//! work so observers match [`beskid_pipeline::phases::JIT_RUN_PHASE_ORDER`] when mods are active.

pub mod codegen_input;
pub mod aggregate_static;
pub mod closure_static;
pub mod cranelift_host;
pub mod diagnostics;
pub mod errors;
pub mod isle_adapter;
mod isle_trace;
pub mod linking;
pub mod lowering;
pub mod module_emission;
pub mod prepared_syntax;
pub mod services;

pub use codegen_input::{CodegenInput, CodegenInputError};
pub use aggregate_static::{ABI_V5_MANAGED_OBJECT_ALLOCATE, AggregateStaticField, AggregateStaticPlan, emit_aggregate_static_data};
pub use closure_static::{
    ABI_V5_CLOSURE_CAPTURE_STORE, ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE,
    ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT, ClosureCaptureStaticField, ClosureLoweringAuthority,
    ClosureRootAuthority, ClosureStaticDataHandles, ClosureStaticPlan, RuntimeRootContext,
    emit_closure_static_data,
};
pub use diagnostics::{codegen_error_to_diagnostic, codegen_errors_to_diagnostics};
pub use errors::{CodegenError, RETIRED_HIR_LOWERING_PATH};
pub use isle_adapter::{
    ItemModuleImporter, SyntaxNodeFacts, emit_isle_closure_lambda_entry, emit_isle_expression,
    emit_isle_expression_with_call_importer, emit_isle_item, emit_isle_item_with_call_importer,
    emit_isle_item_with_services, emit_isle_item_with_services_specialization,
    syntax_item_signature,
};
pub use linking::{
    FunctionDefIndex, LinkPlan, LinkSymbol, MissingSymbol, referenced_extern_imports,
    validate_artifact,
};
pub use lowering::{
    CodegenArtifact, CodegenContext, CodegenResult, DYNAMIC_TYPE_NAME, ExportEntry, ExternImport,
    LoweredFunction, dynamic_clif_type, is_dynamic_type_id, lower_node, lower_program,
    map_type_id_to_clif_with_dynamic, mapping_pair_eligible, object_link_symbol, pointer_type,
    require_mapping_eligible, shape_id_for_item,
};
pub use module_emission::{
    DescriptorHandles, SyntaxModuleItem, emit_closure_static_plans, emit_string_literals,
    emit_type_descriptors, lower_syntax_program,
};
pub use prepared_syntax::{
    PreparedSyntaxEntrypoint, lower_canonical_runtime_prepared_syntax,
    lower_prepared_syntax_entrypoint, lower_prepared_syntax_module,
    lower_syntax_assembly_entrypoint,
};
pub use services::{jit_symbol_for_item, materialize_source_path_for_lowering, render_clif};
