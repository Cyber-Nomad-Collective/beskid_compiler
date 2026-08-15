//! Syntax-only multi-module project assembly from [`CompilePlan`] and materialized source roots.

mod discovery;
mod loader;
mod module_index;
mod roots;
mod unit_builder;
mod unit_cache;

pub use discovery::{module_path_exists_on_disk, module_path_to_relative_path, resolve_module_file};
pub(crate) use loader::assemble_program;
pub use loader::assemble_program_with_materializer;
pub use loader::{assembly_options_for_plan, assembly_options_for_prepare, AssemblyError, UnitMaterializer};
pub use module_index::{infer_logical_module_path, AssemblyModule, ModuleGraph, ModuleIndex};
pub use roots::{
    effective_roots_for_plan, effective_roots_from_lockfile, effective_roots_from_plan_and_workspace,
    module_roots_from_effective, EffectiveCompilationRoots, RootEntry,
};
pub use unit_builder::UnitBuilder;
pub use unit_cache::{
    cache_root_for_project, disk_cache_stats, ensure_manifest, unit_content_fingerprint, unit_fingerprint,
    UnitCacheStats,
};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::projects::AssemblyDiscovery;
use crate::syntax::{Program, Spanned, SyntaxGenerationId};
use crate::syntax_query::SyntaxIndex;

/// One parsed and macro-expanded compilation unit.
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub logical_name: String,
    pub path: PathBuf,
    pub source: String,
    pub program: Spanned<Program>,
}

/// Generation-bound syntax project shared by analysis and IDE query boundaries.
#[derive(Clone)]
pub struct ProgramAssembly {
    pub roots: EffectiveCompilationRoots,
    pub units: Arc<Vec<SourceUnit>>,
    /// Syntax indexes in exactly the same order as `units`.
    pub syntax_indexes: Arc<Vec<SyntaxIndex>>,
    pub generation: SyntaxGenerationId,
    pub entry_index: usize,
    pub discovery: AssemblyDiscovery,
    pub module_index: Arc<ModuleIndex>,
    pub has_std_dependency: bool,
    /// Physical units copied from compiler-owned Foundation sources by workspace materialization.
    pub trusted_corelib_service_paths: Arc<[PathBuf]>,
}

impl std::fmt::Debug for ProgramAssembly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramAssembly")
            .field("units", &self.units.len())
            .field("syntax_indexes", &self.syntax_indexes.len())
            .field("generation", &self.generation)
            .field("entry_index", &self.entry_index)
            .field("discovery", &self.discovery)
            .field("has_std_dependency", &self.has_std_dependency)
            .field("trusted_corelib_service_paths", &self.trusted_corelib_service_paths.len())
            .finish()
    }
}

impl ProgramAssembly {
    pub fn new(
        roots: EffectiveCompilationRoots,
        units: Arc<Vec<SourceUnit>>,
        entry_index: usize,
        discovery: AssemblyDiscovery,
        module_index: Arc<ModuleIndex>,
        has_std_dependency: bool,
        generation: SyntaxGenerationId,
    ) -> Self {
        let syntax_indexes =
            Arc::new(units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>());
        Self {
            roots,
            units,
            syntax_indexes,
            generation,
            entry_index,
            discovery,
            module_index,
            has_std_dependency,
            trusted_corelib_service_paths: Arc::from([]),
        }
    }

    pub fn entry_unit(&self) -> &SourceUnit {
        &self.units[self.entry_index]
    }

    pub fn entry_syntax_index(&self) -> &SyntaxIndex {
        &self.syntax_indexes[self.entry_index]
    }

    pub fn syntax_index_for_path(&self, path: &Path) -> Option<&SyntaxIndex> {
        self.units
            .iter()
            .position(|unit| crate::paths::same_file(&unit.path, path))
            .and_then(|index| self.syntax_indexes.get(index))
    }

    /// Rebind the entry unit when reusing a workspace-wide assembly for another target file.
    pub fn with_entry_at(&self, entry_path: &Path) -> Option<Self> {
        let target = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());
        let entry_index = self
            .units
            .iter()
            .position(|unit| unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone()) == target)?;
        Some(Self { entry_index, ..self.clone() })
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        roots::module_roots_from_effective(&self.roots)
    }
}
