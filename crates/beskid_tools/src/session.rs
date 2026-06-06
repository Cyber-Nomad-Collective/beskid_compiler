//! Facade for resolve + semantic gate flows shared by compile commands.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{self, PreparedCompilation, ResolvedInput};
use beskid_pipeline::PipelineObserver;

use crate::pipeline::{CliPipeline, PipelineProgressKind, frontend, use_cli_spinner};

/// Project / lockfile inputs for [`CommandSession::resolve_input`].
#[derive(Debug, Clone, Copy)]
pub struct ResolveInputArgs<'a> {
    pub input: Option<&'a PathBuf>,
    pub project: Option<&'a PathBuf>,
    pub target: Option<&'a str>,
    pub workspace_member: Option<&'a str>,
    pub frozen: bool,
    pub locked: bool,
}

/// Options for [`CommandSession::semantic_gate`].
#[derive(Debug, Clone, Copy)]
pub struct SemanticGateOptions {
    /// When true, stop animated progress after analysis (run / test / clif).
    pub finish_prepare_ui: bool,
    pub prepare_message: &'static str,
}

impl Default for SemanticGateOptions {
    fn default() -> Self {
        Self {
            finish_prepare_ui: true,
            prepare_message: "Analysis complete",
        }
    }
}

/// Shared CLI session: pipeline observer plus resolve and semantic gate helpers.
pub struct CommandSession {
    pipeline: Arc<CliPipeline>,
}

impl CommandSession {
    /// Create a session with pipeline progress for the given phase budget.
    pub fn with_progress(plain: bool, kind: PipelineProgressKind) -> Self {
        Self {
            pipeline: Arc::new(CliPipeline::new_with_kind(use_cli_spinner(plain), kind)),
        }
    }

    /// Resolve project input while forwarding pipeline events to this session.
    pub fn resolve_input(&self, args: &ResolveInputArgs<'_>) -> Result<ResolvedInput> {
        frontend::resolve_input_with_pipeline(
            args.input,
            args.project,
            args.target,
            args.workspace_member,
            args.frozen,
            args.locked,
            Some(self.pipeline.as_ref()),
        )
    }

    /// Open a session, resolve, and return both (common compile-command entry).
    pub fn open_and_resolve(
        plain: bool,
        kind: PipelineProgressKind,
        args: &ResolveInputArgs<'_>,
    ) -> Result<(Self, ResolvedInput)> {
        let session = Self::with_progress(plain, kind);
        let resolved = session.resolve_input(args)?;
        Ok((session, resolved))
    }

    pub fn pipeline(&self) -> &CliPipeline {
        &self.pipeline
    }

    pub fn pipeline_arc(&self) -> Arc<CliPipeline> {
        Arc::clone(&self.pipeline)
    }

    pub fn observer(&self) -> &dyn PipelineObserver {
        self.pipeline.as_ref()
    }

    /// Run the semantic diagnostics gate; halts progress bars before printing diagnostics.
    pub fn semantic_gate(
        &self,
        resolved: &ResolvedInput,
        options: SemanticGateOptions,
    ) -> Result<()> {
        let _ = self.semantic_gate_prepared(resolved, options)?;
        Ok(())
    }

    /// Like [`semantic_gate`] but returns the prepared compilation snapshot (for `beskid test`).
    pub fn semantic_gate_prepared(
        &self,
        resolved: &ResolvedInput,
        options: SemanticGateOptions,
    ) -> Result<PreparedCompilation> {
        self.pipeline.halt_progress_bars_for_output();

        let (prepared, gate_diagnostics) = self.run_semantic_diagnostics(resolved)?;
        self.finish_gate(&gate_diagnostics, options)?;
        Ok(prepared)
    }

    fn run_semantic_diagnostics(
        &self,
        resolved: &ResolvedInput,
    ) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>)> {
        beskid_queries::prepare_compilation_diagnostics(
            resolved,
            services::PrepareOptions {
                mode: services::PrepareMode::DiagnosticsOnly,
                front_end: services::FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
            },
            Some(self.pipeline.as_ref()),
        )
    }

    fn finish_gate(
        &self,
        gate_diagnostics: &[SemanticDiagnostic],
        options: SemanticGateOptions,
    ) -> Result<()> {
        self.pipeline.report_semantic_diagnostics(gate_diagnostics);
        services::require_no_semantic_errors(gate_diagnostics).map_err(anyhow::Error::from)?;

        if options.finish_prepare_ui {
            self.pipeline.finish_prepare_ui(options.prepare_message);
        }

        Ok(())
    }
}
