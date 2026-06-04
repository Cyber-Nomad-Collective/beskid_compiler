//! Compiler mod host orchestration: discovery, descriptor loading, registration
//! validation, contract dispatch through `mod.collect`/`mod.generate`/`mod.analyze`/
//! `mod.rewrite`, and pipeline emission.
//!
//! See `site/website/src/content/docs/platform-spec/compiler/compiler-mods/`.

mod analyze;
mod api;
mod capabilities;
mod collect;
pub mod diagnostics;
mod discovery;
mod generate;
pub mod invoker;
mod load;
mod merge;
mod query_bridge;
mod reparse;
mod rewrite;
mod types;
mod validate;

pub use api::{
    extract_mod_host_diagnostics, run_analyze_rewrite, run_analyze_rewrite_after_composition,
    run_analyze_rewrite_with_invoker, run_through_generate,
};
pub use diagnostics::{ModHostDiagnostics, ModHostIssue};
pub use invoker::{
    AnalyzerDiagnostic, AnalyzerOutcome, AnalyzerSeverity, CollectorOutcome,
    ContractInvocationError, ContractInvoker, GeneratorOutcome, InvocationKind, RewriterOutcome,
    ScriptedContractInvoker, StubContractInvoker,
};
pub use query_bridge::{
    PipelineOp, PipelineOpKind, PipelineValidationError, QueryBounds, SdkNodeRef, SdkNodeSpan,
    SdkSyntaxPipeline, SdkSyntaxQuery, SdkSyntaxSelection, downcast_node, materialize_snapshot,
    query_at,
};
pub use types::{
    ContractRegistration, ModArtifactDescriptor, ModHostAnalyzeResult, ModHostGenerateResult,
    ModHostInput, ModHostSession,
};
