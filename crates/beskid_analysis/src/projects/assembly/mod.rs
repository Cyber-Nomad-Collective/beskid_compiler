//! Multi-module program assembly from [`CompilePlan`] and materialized source roots.

mod discovery;
mod loader;
mod module_index;
mod roots;

pub use discovery::{
    module_path_exists_on_disk, module_path_to_relative_path, resolve_module_file,
};
pub use loader::{AssemblyError, assemble_program};
pub use module_index::ModuleIndex;
pub use roots::{
    EffectiveCompilationRoots, RootEntry, effective_roots_for_plan, effective_roots_from_lockfile,
    effective_roots_from_plan_and_workspace, module_roots_for_plan, module_roots_from_effective,
};

use std::path::PathBuf;

use crate::projects::AssemblyDiscovery;
use crate::syntax::{Program, Spanned};

/// One parsed compilation unit.
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub logical_name: String,
    pub path: PathBuf,
    pub source: String,
    pub program: Spanned<Program>,
}

/// Assembled multi-module view shared by analyze, LSP, and lowering.
#[derive(Debug, Clone)]
pub struct ProgramAssembly {
    pub roots: EffectiveCompilationRoots,
    pub units: Vec<SourceUnit>,
    pub entry_index: usize,
    pub discovery: AssemblyDiscovery,
    pub module_index: ModuleIndex,
}

impl ProgramAssembly {
    pub fn entry_unit(&self) -> &SourceUnit {
        &self.units[self.entry_index]
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        roots::module_roots_from_effective(&self.roots)
    }
}
