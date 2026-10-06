//! Cranelift-based code generation for Beskid.

#![allow(clippy::drop_non_drop, clippy::question_mark, clippy::result_large_err)]
//!
//! Pipeline: [`lower_prepared_syntax_entrypoint`] reports the generated syntax-codegen boundary
//! before `codegen_clif`. Mod orchestration ids (`mod.load` … `mod.rewrite`,
//! [`beskid_pipeline::phases::SYNTAX_GENERATION`]) are **not** emitted here; they are reserved for
//! hosts that run the mod SDK and should call [`beskid_pipeline::observe_phase`] around real mod
//! work so observers match [`beskid_pipeline::phases::JIT_RUN_PHASE_ORDER`] when mods are active.

#[cfg(test)]
extern crate self as beskid_codegen;

pub mod aggregate_static;
pub mod array_static;
pub mod artifact;
mod artifact_validation;
pub mod backend;
pub mod glue;
pub mod closure_static;
pub mod codegen_input;
pub mod dynamic_shape;
pub mod dynamic_initialization;
pub mod cranelift_host;
pub mod event_handler_static;
pub mod isle_adapter;
mod isle_trace;
pub mod module_emission;
pub mod native_mod;
pub mod prepared_syntax;
pub mod reserved_failure;
pub mod checked_effect;
pub mod services;

pub use aggregate_static::{
    ABI_V5_MANAGED_OBJECT_ALLOCATE, AggregateObjectLayout, AggregateStaticField, AggregateStaticPlan,
    emit_aggregate_static_data,
};
pub use array_static::{
    ABI_V5_ARRAY_ALLOCATE_ROOTED, ABI_V5_ARRAY_CONSTRUCTION_FINISH, ArrayStaticPlan, emit_array_static_data,
};
pub use artifact::{
    CodegenArtifact, CodegenContext, ExportEntry, ExternImport, LoweredFunction, TypeDescriptorData,
    internal_link_symbol, object_link_symbol,
};
pub use artifact_validation::{
    MissingSymbol, referenced_extern_imports, referenced_trusted_extern_imports, validate_artifact,
};
pub use closure_static::{
    ABI_V5_CLOSURE_CAPTURE_STORE, ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE, ClosureCaptureStaticField,
    ClosureLoweringAuthority, ClosureStaticDataHandles, ClosureStaticPlan, RuntimeRootContext,
    emit_closure_static_data,
};
pub use codegen_input::{CodegenInput, CodegenInputError, SchedulerCompilerOperation};
pub use event_handler_static::{
    EVENT_HANDLER_ALLOCATION_REQUEST_SYMBOL, EVENT_HANDLER_CODE_OFFSET, EVENT_HANDLER_DESCRIPTOR_SYMBOL,
    EVENT_HANDLER_ENVIRONMENT_OFFSET, EVENT_HANDLER_POINTER_MAP_SYMBOL, emit_event_handler_static_data,
};
pub use isle_adapter::{
    ItemModuleImporter, SyntaxNodeFacts, emit_isle_closure_lambda_entry, emit_isle_expression,
    emit_isle_expression_with_call_importer, emit_isle_item, emit_isle_item_with_call_importer,
    emit_isle_item_with_services, emit_isle_item_with_services_specialization, syntax_item_signature,
};

pub use module_emission::{
    DescriptorHandles, ModuleEmissionSession, SyntaxModuleItem, emit_closure_static_plans, emit_string_literals,
    emit_syntax_program_in_session, emit_type_descriptors, lower_syntax_program,
};
pub use prepared_syntax::{
    PreparedSyntaxEntry, PreparedSyntaxEntrypoint, PreparedSyntaxEntrypoints, lower_canonical_runtime_prepared_syntax,
    lower_prepared_syntax_entrypoint, lower_prepared_syntax_entrypoints, lower_prepared_syntax_module,
    lower_prepared_syntax_library, lower_prepared_syntax_selected_callables,
    lower_prepared_syntax_module_with_callables, PreparedSyntaxCallable, PreparedSyntaxModule,
    lower_syntax_assembly_entrypoint, with_prepared_glue_input,
};
pub use services::{materialize_source_path_for_lowering, render_clif};

pub use native_mod::producer::{PreparedNativeMod, PreparedNativeModEntry, lower_prepared_native_mod};

mod stable_shape;
pub use stable_shape::CompiledStableShape;

pub mod checked_callback;

pub mod serialization_shape;

pub mod serialization_recipe;
