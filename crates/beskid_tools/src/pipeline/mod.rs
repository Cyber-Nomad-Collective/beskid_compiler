//! CLI input resolution through the line-oriented [`CliPipeline`] observer.

pub mod frontend;
mod labels;
mod output;
pub mod resolve_options;
pub mod tui;

use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::services::{ResolvedInput, ResolvedProject};
pub use output::{CliPipeline, PROGRESS_DELAY, PipelineProgressKind, progress_due, use_cli_spinner};
pub use resolve_options::{
    CliInputPipelineOptions, CliProjectPipelineOptions, CliResolveOptions, FrontendProjectPipelineOptions,
};

pub fn resolve_input_with_cli_pipeline(options: CliResolveOptions<'_>) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    resolve_input_with_cli_pipeline_kind(CliInputPipelineOptions {
        resolve: options,
        progress_kind: PipelineProgressKind::FullBuild,
    })
}

pub fn resolve_input_with_cli_pipeline_kind(
    options: CliInputPipelineOptions<'_>,
) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    let CliInputPipelineOptions { resolve, progress_kind } = options;
    let pipeline = Arc::new(CliPipeline::new_with_kind(use_cli_spinner(resolve.plain), progress_kind));
    let resolved = frontend::resolve_input_with_pipeline(resolve, Some(pipeline.as_ref()))?;
    Ok((pipeline, resolved))
}

pub fn resolve_project_with_cli_pipeline(
    options: CliProjectPipelineOptions<'_>,
) -> Result<(Arc<CliPipeline>, ResolvedProject)> {
    let CliProjectPipelineOptions { resolve, unresolved_dependency_policy } = options;
    let pipeline =
        Arc::new(CliPipeline::new_with_kind(use_cli_spinner(resolve.plain), PipelineProgressKind::FullBuild));
    let resolved = frontend::resolve_project_with_pipeline(FrontendProjectPipelineOptions {
        resolve,
        unresolved_dependency_policy,
        pipeline: Some(pipeline.as_ref()),
    })?;
    Ok((pipeline, resolved))
}
