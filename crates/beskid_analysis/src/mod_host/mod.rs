//! Compiler mod host orchestration: discovery, descriptor loading, phase emission, and MVP hooks.

mod analyze;
mod api;
mod capabilities;
mod collect;
mod discovery;
mod generate;
mod load;
mod merge;
mod query_bridge;
mod reparse;
mod rewrite;
mod types;

pub use api::{run_analyze_rewrite, run_through_generate};
pub use query_bridge::{
    PipelineOp, PipelineOpKind, PipelineValidationError, QueryBounds, SdkNodeRef, SdkNodeSpan,
    SdkSyntaxPipeline, SdkSyntaxQuery, SdkSyntaxSelection, downcast_node, materialize_snapshot,
    query_at,
};
pub use types::{
    ContractRegistration, ModArtifactDescriptor, ModHostGenerateResult, ModHostInput,
    ModHostSession,
};
