//! Facade for resolve + executable prepare gate flows shared by compile commands.
//!
//! Compile commands should perform **one** [`executable_gate_prepared`] prepare before lowering or
//! codegen. Chaining the deprecated semantic gate APIs with a separate lower prepare duplicates
//! pipeline phases and skews [`PipelineProgressKind::PrepareAndRun`] progress.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{self, PreparedCompilation, ResolvedInput};
use beskid_pipeline::PipelineObserver;

use crate::pipeline::{CliPipeline, CliResolveOptions, PipelineProgressKind, frontend, use_cli_spinner};

/// Project / lockfile inputs for [`CommandSession::resolve_input`].
#[derive(Debug, Clone, Copy)]
pub struct ResolveInputArgs<'a> {
    pub input: Option<&'a PathBuf>,
    pub project: Option<&'a PathBuf>,
    pub target: Option<&'a str>,
    pub workspace_member: Option<&'a str>,
    pub frozen: bool,
    pub locked: bool,
    pub offline: bool,
}

/// Options for [`CommandSession::executable_gate_prepared`].
#[derive(Debug, Clone, Copy)]
pub struct SemanticGateOptions {
    /// When true, stop animated progress after analysis (run / test / clif).
    pub finish_prepare_ui: bool,
    pub prepare_message: &'static str,
}

impl Default for SemanticGateOptions {
    fn default() -> Self {
        Self { finish_prepare_ui: true, prepare_message: "Analysis complete" }
    }
}

/// Shared CLI session: pipeline observer plus resolve and executable prepare gate helpers.
pub struct CommandSession {
    mod_host: beskid_analysis::mod_host::ModHost,
    pipeline: Arc<CliPipeline>,
    /// Diagnostics of the most recent executable gate, kept for machine-readable (`--json`) output.
    gate_diagnostics: std::sync::Mutex<Vec<SemanticDiagnostic>>,
}

impl CommandSession {
    /// Create a session with pipeline progress for the given phase budget.
    ///
    /// Use [`PipelineProgressKind::PrepareAndRun`] for run / test / clif paths that call
    /// [`executable_gate_prepared`] once, then lower or JIT-run. The session starts without a native
    /// Mod host; a command that prepares plans with Mod dependencies calls [`Self::set_mod_host`].
    pub fn with_progress(plain: bool, kind: PipelineProgressKind) -> Self {
        Self {
            mod_host: beskid_analysis::mod_host::ModHost::NoMods,
            pipeline: Arc::new(CliPipeline::new_with_kind(use_cli_spinner(plain), kind)),
            gate_diagnostics: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Run the native Mods of this session's preparations in `mod_host`.
    pub fn set_mod_host(&mut self, mod_host: beskid_analysis::mod_host::ModHost) {
        self.mod_host = mod_host;
    }

    /// Resolve project input while forwarding pipeline events to this session.
    pub fn resolve_input(&self, args: &ResolveInputArgs<'_>) -> Result<ResolvedInput> {
        frontend::resolve_input_with_pipeline(
            CliResolveOptions {
                input: args.input,
                project: args.project,
                target: args.target,
                workspace_member: args.workspace_member,
                frozen: args.frozen,
                locked: args.locked,
                offline: args.offline,
                refresh_lock: false,
                plain: false,
            },
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

    /// Single prepare through typed executable HIR with semantic diagnostics gate.
    ///
    /// Primary API for `beskid run`, `beskid build`, `beskid test`, and `beskid dev syntax clif`. Pass the
    /// returned [`PreparedCompilation`] to [`PreparedCompilation::into_executable`] and the
    /// syntax-owned lowering boundary — do not run a second prepare.
    pub fn executable_gate_prepared(
        &self,
        resolved: &ResolvedInput,
        options: SemanticGateOptions,
    ) -> Result<PreparedCompilation> {
        let (prepared, gate_diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics(
            resolved,
            services::PrepareOptions {
                front_end: services::FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                native_mod_adapter_sources: false,
                mod_host: self.mod_host.clone(),
            },
            Some(self.pipeline.as_ref()),
        )?;
        self.finish_gate(&gate_diagnostics, options)?;
        Ok(prepared)
    }

    /// **Deprecated:** use [`executable_gate_prepared`] instead.
    ///
    /// Thin wrapper that forwards to [`executable_gate_prepared`] without a second prepare.
    pub fn semantic_gate_prepared(
        &self,
        resolved: &ResolvedInput,
        options: SemanticGateOptions,
    ) -> Result<PreparedCompilation> {
        self.executable_gate_prepared(resolved, options)
    }

    /// **Deprecated:** use [`executable_gate_prepared`] instead.
    ///
    /// Thin wrapper that forwards to [`executable_gate_prepared`] without a second prepare.
    pub fn semantic_gate(&self, resolved: &ResolvedInput, options: SemanticGateOptions) -> Result<()> {
        let _ = self.executable_gate_prepared(resolved, options)?;
        Ok(())
    }

    /// Diagnostics reported by the most recent executable gate, including warnings of a passing gate
    /// and the errors of a failing one.
    pub fn gate_diagnostics(&self) -> Vec<SemanticDiagnostic> {
        self.gate_diagnostics.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    fn finish_gate(&self, gate_diagnostics: &[SemanticDiagnostic], options: SemanticGateOptions) -> Result<()> {
        *self.gate_diagnostics.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = gate_diagnostics.to_vec();
        self.pipeline.report_semantic_diagnostics(gate_diagnostics);
        services::require_no_semantic_errors(gate_diagnostics).map_err(anyhow::Error::from)?;

        if options.finish_prepare_ui {
            self.pipeline.finish_prepare_ui(options.prepare_message);
        }

        Ok(())
    }
}
