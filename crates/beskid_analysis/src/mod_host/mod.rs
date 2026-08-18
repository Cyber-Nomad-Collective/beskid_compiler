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
pub mod diagnostics;
mod discovery;
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
mod types;
mod validate;

pub use api::{
    collect_mod_target_fingerprint, extract_mod_host_diagnostics, native_invoker_for_plan, run_analyze_rewrite,
    run_analyze_rewrite_after_composition, run_analyze_rewrite_with_invoker, run_through_generate,
};
pub use collect::{capture_target_fingerprint, targets_changed};
pub use context::ModInvocationContext;
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
