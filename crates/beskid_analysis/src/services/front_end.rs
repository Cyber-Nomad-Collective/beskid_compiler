//! Shared front-end spine: assembly, parse, mods, semantic gate, HIR with module index.

use std::path::Path;

use anyhow::Result;
use beskid_pipeline::PipelineObserver;

use crate::projects::{CompilePlan, PreparedProjectWorkspace};

use super::input::ResolvedInput;
use super::prepare::{PrepareOptions, prepare_compilation};

/// Result of the shared front-end through typed HIR (codegen consumes this).
pub struct FrontEndTypedResult {
    pub assembly: crate::projects::ProgramAssembly,
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    pub hir: crate::syntax::Spanned<crate::hir::HirProgram>,
    pub resolution: crate::resolve::Resolution,
    pub typed: crate::types::TypeResult,
    pub binding_plan: crate::composition::BindingPlan,
    pub composition_snapshot: crate::composition::CompositionSnapshot,
}

impl std::fmt::Debug for FrontEndTypedResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrontEndTypedResult")
            .field("assembly", &self.assembly)
            .field("resolution", &self.resolution)
            .field("typed", &self.typed)
            .finish_non_exhaustive()
    }
}

impl FrontEndTypedResult {
    /// Borrowed view for per-entrypoint lowering (shared assembly/resolution/typed HIR).
    pub fn as_lower_input(&self) -> FrontEndLowerInput<'_> {
        FrontEndLowerInput {
            assembly: &self.assembly,
            hir: &self.hir,
            resolution: &self.resolution,
            typed: &self.typed,
        }
    }
}

/// Borrowed front-end bundle passed into codegen lowering.
pub struct FrontEndLowerInput<'a> {
    pub assembly: &'a crate::projects::ProgramAssembly,
    pub hir: &'a crate::syntax::Spanned<crate::hir::HirProgram>,
    pub resolution: &'a crate::resolve::Resolution,
    pub typed: &'a crate::types::TypeResult,
}

/// Options for [`compile_front_end_with_pipeline`].
#[derive(Debug, Clone)]
pub struct FrontEndOptions {
    pub with_semantic_diagnostics: bool,
    pub assembly_discovery: crate::projects::AssemblyDiscovery,
    pub module_level_meta_items_allowed: Option<bool>,
}

impl Default for FrontEndOptions {
    fn default() -> Self {
        Self {
            with_semantic_diagnostics: true,
            assembly_discovery: crate::projects::AssemblyDiscovery::ImportClosure,
            module_level_meta_items_allowed: None,
        }
    }
}

/// Assemble, run mod host + semantic gate, and lower the entry unit with cross-module resolution.
pub fn compile_front_end_with_pipeline(
    entry_path: &Path,
    entry_source: &str,
    compile_plan: Option<&CompilePlan>,
    prepared_workspace: Option<&PreparedProjectWorkspace>,
    options: FrontEndOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<FrontEndTypedResult> {
    let plan = compile_plan.ok_or_else(|| {
        anyhow::anyhow!("compile_front_end requires a CompilePlan (project context)")
    })?;

    let resolved = super::prepare::resolved_input_from_plan(
        entry_path.to_path_buf(),
        entry_source.to_string(),
        plan.clone(),
        prepared_workspace.cloned(),
        None,
    );

    let prepared = prepare_compilation(
        &resolved,
        PrepareOptions { front_end: options },
        pipeline,
    )?;

    prepared.into_executable()
}
