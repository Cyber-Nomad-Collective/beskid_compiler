//! Shared CLI resolution option structs for pipeline front-end helpers.

use std::path::PathBuf;

use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_pipeline::PipelineObserver;

use super::PipelineProgressKind;

/// Project / workspace / lockfile flags shared by CLI resolution entry points.
pub struct CliResolveOptions<'a> {
    pub input: Option<&'a PathBuf>,
    pub project: Option<&'a PathBuf>,
    pub target: Option<&'a str>,
    pub workspace_member: Option<&'a str>,
    pub frozen: bool,
    pub locked: bool,
    pub plain: bool,
}

impl<'a> CliResolveOptions<'a> {
    pub fn new(
        input: Option<&'a PathBuf>,
        project: Option<&'a PathBuf>,
        target: Option<&'a str>,
        workspace_member: Option<&'a str>,
        frozen: bool,
        locked: bool,
        plain: bool,
    ) -> Self {
        Self {
            input,
            project,
            target,
            workspace_member,
            frozen,
            locked,
            plain,
        }
    }
}

/// Resolve input through the CLI pipeline with a chosen progress kind.
pub struct CliInputPipelineOptions<'a> {
    pub resolve: CliResolveOptions<'a>,
    pub progress_kind: PipelineProgressKind,
}

/// Resolve a project through the CLI pipeline.
pub struct CliProjectPipelineOptions<'a> {
    pub resolve: CliResolveOptions<'a>,
    pub unresolved_dependency_policy: UnresolvedDependencyPolicy,
}

/// Resolve a project with an optional pipeline observer (frontend layer).
pub struct FrontendProjectPipelineOptions<'a> {
    pub resolve: CliResolveOptions<'a>,
    pub unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pub pipeline: Option<&'a dyn PipelineObserver>,
}
