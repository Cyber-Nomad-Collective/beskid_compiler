use std::path::{Path, PathBuf};

use thiserror::Error;

use super::super::SourceUnit;
use crate::projects::CompilePlan;
use crate::projects::model::{AssemblyDiscovery, AssemblyOptions};
use crate::syntax::SyntaxGenerationId;
use crate::syntax::{Program, Spanned};
use crate::syntax_query::SyntaxIndex;

/// Optional Salsa-backed unit builder (set by `beskid_queries` during assembly).
pub type UnitMaterializer = std::sync::Arc<
    dyn Fn(&Path, &str, SyntaxGenerationId) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> + Send + Sync,
>;

#[derive(Debug, Error)]
pub enum AssemblyError {
    #[error("failed to read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("entry file not found under effective roots: {path}")]
    EntryNotFound { path: PathBuf },
    #[error("assembly exceeded max_units ({max})")]
    MaxUnits { max: usize },
}

pub(crate) fn expand_syntax_for_assembly(program: Spanned<Program>) -> Spanned<Program> {
    crate::macros::expand_program_with_diagnostics(program, crate::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH, "", "")
        .program
}

/// Default assembly options for a compile plan.
///
/// Targets with an explicit `entry` (Lib/App test entrypoints) use import-closure discovery;
/// aggregate / IDE-style plans without `entry` scan the workspace.
pub fn assembly_options_for_plan(plan: &CompilePlan) -> AssemblyOptions {
    let mut options = AssemblyOptions::default();
    if plan.target.entry.as_deref().unwrap_or("").trim().is_empty() {
        options.discovery = AssemblyDiscovery::WorkspaceScan;
    } else {
        options.discovery = AssemblyDiscovery::ImportClosure;
    }
    options
}

/// Merge plan-derived discovery with an explicit front-end override.
///
/// [`AssemblyDiscovery::ImportClosure`] in `front_end_discovery` means "use the plan default"
/// (import closure when `entry` is set, workspace scan when it is not). Any other mode overrides.
pub fn assembly_options_for_prepare(plan: &CompilePlan, front_end_discovery: AssemblyDiscovery) -> AssemblyOptions {
    let mut options = assembly_options_for_plan(plan);
    if front_end_discovery != AssemblyDiscovery::ImportClosure {
        options.discovery = front_end_discovery;
    }
    options
}
