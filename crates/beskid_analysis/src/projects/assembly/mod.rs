//! Multi-module program assembly from [`CompilePlan`] and materialized source roots.

mod discovery;
mod hir_units;
mod loader;
mod module_index;
mod roots;
mod unit_builder;
mod unit_cache;

pub use discovery::{
    module_path_exists_on_disk, module_path_to_relative_path, resolve_module_file,
};
pub use hir_units::{UnitHir, build_hir_units, reindex_hir_units_in_place};
pub use loader::{
    AssemblyError, UnitMaterializer, assembly_options_for_plan, assembly_options_for_prepare,
};
pub use loader::assemble_program_with_materializer;
pub(crate) use loader::assemble_program;
pub use module_index::{ModuleIndex, infer_logical_module_path};
pub use roots::{
    EffectiveCompilationRoots, RootEntry, effective_roots_for_plan, effective_roots_from_lockfile,
    effective_roots_from_plan_and_workspace, module_roots_from_effective,
};
pub use unit_builder::UnitBuilder;
pub use unit_cache::{
    UnitCacheStats, cache_root_for_project, disk_cache_stats, ensure_manifest,
    unit_content_fingerprint, unit_fingerprint,
};

use std::path::PathBuf;
use std::sync::Arc;

use crate::hir::HirProgram;
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
pub struct ProgramAssembly {
    pub roots: EffectiveCompilationRoots,
    pub units: Arc<Vec<SourceUnit>>,
    /// HIR lowered once per unit (same order as `units`).
    pub hir_units: Arc<Vec<UnitHir>>,
    pub entry_index: usize,
    pub discovery: AssemblyDiscovery,
    pub module_index: Arc<ModuleIndex>,
    pub has_std_dependency: bool,
}

impl std::fmt::Debug for ProgramAssembly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramAssembly")
            .field("units", &self.units.len())
            .field("hir_units", &self.hir_units.len())
            .field("entry_index", &self.entry_index)
            .field("discovery", &self.discovery)
            .field("has_std_dependency", &self.has_std_dependency)
            .finish()
    }
}

impl Clone for ProgramAssembly {
    fn clone(&self) -> Self {
        Self {
            roots: self.roots.clone(),
            units: Arc::clone(&self.units),
            hir_units: Arc::clone(&self.hir_units),
            entry_index: self.entry_index,
            discovery: self.discovery,
            module_index: Arc::clone(&self.module_index),
            has_std_dependency: self.has_std_dependency,
        }
    }
}

impl ProgramAssembly {
    pub fn entry_unit(&self) -> &SourceUnit {
        &self.units[self.entry_index]
    }

    pub fn entry_hir(&self) -> &Spanned<HirProgram> {
        &self.hir_units[self.entry_index].hir
    }

    /// Non-entry unit HIR for normalization and dependency-aware type checking.
    pub fn dependency_hir_refs(&self) -> Vec<&Spanned<HirProgram>> {
        self.hir_units
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.entry_index)
            .map(|(_, unit)| &unit.hir)
            .collect()
    }

    /// Rebind the entry unit when reusing a workspace-wide assembly for another target file.
    pub fn with_entry_at(&self, entry_path: &std::path::Path) -> Option<Self> {
        let target = entry_path
            .canonicalize()
            .unwrap_or_else(|_| entry_path.to_path_buf());
        let entry_index = self.units.iter().position(|unit| {
            unit.path
                .canonicalize()
                .unwrap_or_else(|_| unit.path.clone())
                == target
        })?;
        Some(Self {
            entry_index,
            ..self.clone()
        })
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        roots::module_roots_from_effective(&self.roots)
    }
}
