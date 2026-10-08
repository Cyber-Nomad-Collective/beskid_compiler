//! Compiler mod host orchestration: discovery, descriptor loading, registration
//! validation, contract dispatch through `mod.collect`/`mod.generate`/`mod.analyze`/
//! `mod.rewrite`, and pipeline emission.
//!
//! See `site/website/src/content/docs/platform-spec/compiler/compiler-mods/`.

mod analyze;
mod api;
mod capabilities;
mod code_string;
mod collect;
mod context;
mod descriptor;
pub mod diagnostics;
mod discovery;
pub use descriptor::{
    NATIVE_MOD_BUILD_TOOL_ROLES, NATIVE_MOD_COMPILER_TOOL_ROLE, NATIVE_MOD_DESCRIPTOR_FILE, NativeModCallableIdentity,
    NativeModRuntimeBinding, mod_artifact_inventory, native_mod_artifact_key, native_mod_dependency_identity,
    native_mod_dependency_sources, native_mod_evidence_is_current, native_mod_file_sha256,
    native_mod_inventory_identity, native_mod_runtime_binding, native_mod_runtime_profile,
    read_mod_artifact_descriptor,
};
mod emit_bridge;
mod generate;
mod generate_output;
mod glue;
pub mod invoker;
mod load;
mod merge;
mod native;
mod query_bridge;
mod registrations;
mod reparse;
mod rewrite;
mod semantic;
mod semantic_scope;
mod structural;
mod types;
mod validate;
pub use semantic::*;

pub use api::{
    collect_mod_target_fingerprint, extract_mod_host_diagnostics, native_invoker_for_plan, run_analyze_rewrite,
    run_analyze_rewrite_after_composition, run_analyze_rewrite_with_invoker, run_through_generate,
    run_through_generate_without_materializing_outputs,
};
pub use collect::{capture_target_fingerprint, targets_changed};
pub use context::{ModCollectRequest, ModGenerationRequest, ModInvocationContext};
pub use diagnostics::{
    ModHostDiagnostics, ModHostIssue, SyntaxFix, SyntaxTextEdit, SyntaxTextEditKind, analyzer_diagnostic_to_semantic,
    analyzer_fix_to_syntax_fix,
};
pub use emit_bridge::{
    materialize_contract_definition, materialize_function_definition, materialize_program_item,
    materialize_program_items, materialize_type_definition,
};
pub use generate_output::{
    CodeGenerateOutput, GenerateOutputFile, GenerateOutputLayout, load_generate_output_layout, resolve_generated_path,
    resolve_package_root, write_code_generate_output, write_typed_generate_output,
};
pub use glue::{GlueAnnotation, GlueAttributeKind, collect_glue_annotations, is_glue_attribute};
pub use invoker::{
    AnalyzerDiagnostic, AnalyzerFix, AnalyzerOutcome, AnalyzerSeverity, CollectorOutcome, ContractInvocationError,
    ContractInvoker, GeneratorOutcome, InvocationKind, RewriteEdit, RewriterOutcome, ScriptedContractInvoker,
    StubContractInvoker,
};
pub use native::NativeContractInvoker;
pub use query_bridge::{
    PipelineOp, PipelineOpKind, PipelineValidationError, QueryBounds, SdkNodeRef, SdkNodeSpan, SdkSyntaxPipeline,
    SdkSyntaxQuery, SdkSyntaxSelection, downcast_node, materialize_snapshot, query_at,
};
pub use registrations::{
    extract_mod_contract_registrations, extract_mod_contract_registrations_from_syntax, mod_contract_entry_symbol,
};
pub use types::{
    ContractRegistration, ModArtifactDescriptor, ModHostAnalyzeResult, ModHostGenerateResult, ModHostInput,
    ModHostSession, ProgramItem,
};

pub use structural::{
    StructuralContributionArena, StructuralContributionBounds, StructuralContributionError, StructuralContributionItem,
    StructuralContributionTag, StructuralMaterializedItem, StructuralNodeHandle, StructuralProvenance,
};

pub use semantic_scope::ModSemanticScope;

mod callback_transport;

mod native_wire;
mod native_worker;
#[doc(hidden)]
pub use native_worker::run_native_mod_worker;
mod native_bridge;
mod native_channel;
mod native_correspondence;
mod native_semantic_values;
mod native_services;
#[doc(hidden)]
pub use native_bridge::{NativeInvocationOutput, invoke_qualified_native_rewriter, invoke_qualified_native_transport};

mod syntax_authority;
pub use syntax_authority::*;

mod native_results;
#[doc(hidden)]
pub use native_results::{
    native_analyzer_result, native_attribute_result, native_collector_result, native_generator_result,
};
