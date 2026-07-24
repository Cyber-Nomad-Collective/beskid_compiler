//! Project session identity for analysis and tooling (manifest paths, module roots).
//!
//! [`ProgramAssembly`] and materialized workspace state live in
//! [`beskid_queries::BeskidDatabase`] (see `program_assembly` and prepare spine).

use std::path::{Path, PathBuf};

use crate::projects::{
    CompilePlan, ProgramAssembly, ProjectGraphBuildOptions, ProjectKind, UnresolvedDependencyPolicy,
    build_compile_plan_with_policy_and_graph, discover_workspace_file, effective_roots_for_plan,
    load_manifest_from_path, module_roots_from_effective, resolve_project_manifest_for_source_path,
};

/// Thin project session view: manifest paths and module roots for query/database inputs.
///
/// Does not cache [`ProgramAssembly`] or prepared workspace artifacts. Callers attach a
/// [`beskid_queries::BeskidDatabase`] (or use `beskid_queries::with_db`) when assembly or prepare
/// output is required.
#[derive(Debug, Clone)]
pub struct ProjectSessionHandle {
    pub project_manifest_path: PathBuf,
    pub workspace_manifest_path: Option<PathBuf>,
    pub project_kind: ProjectKind,
    pub compile_plan: Option<CompilePlan>,
    pub module_roots: Vec<PathBuf>,
}

/// Backward-compatible alias for [`ProjectSessionHandle`].
pub type CompilationContext = ProjectSessionHandle;

impl ProjectSessionHandle {
    /// Build a session handle for IDE/analysis paths (module roots from lockfile when present).
    pub fn try_for_analysis_path(path: &Path, workspace_member: Option<&str>) -> Option<Self> {
        Self::try_for_analysis_path_with_graph_options(path, workspace_member, ProjectGraphBuildOptions::default())
    }

    /// Like [`Self::try_for_analysis_path`], but forwards [`ProjectGraphBuildOptions`] (for example
    /// `workspace_member_for_meta_default`) into graph / compile-plan construction.
    pub fn try_for_analysis_path_with_graph_options(
        path: &Path,
        workspace_member: Option<&str>,
        graph_options: ProjectGraphBuildOptions,
    ) -> Option<Self> {
        let (manifest_path, workspace_resolution) =
            resolve_project_manifest_for_source_path(path, workspace_member).ok()??;
        let manifest = load_manifest_from_path(&manifest_path).ok()?;
        let project_kind = manifest.project.kind;
        let workspace_manifest_path = workspace_resolution
            .map(|summary| summary.workspace_manifest_path)
            .or_else(|| discover_workspace_file(&manifest_path));
        let compile_plan = match project_kind {
            ProjectKind::Template | ProjectKind::Bsol => None,
            ProjectKind::Host | ProjectKind::Mod | ProjectKind::Aggregate => build_compile_plan_with_policy_and_graph(
                &manifest_path,
                None,
                UnresolvedDependencyPolicy::Error,
                graph_options,
            )
            .ok(),
        };
        let module_roots = compile_plan
            .as_ref()
            .map(|plan| module_roots_from_effective(&effective_roots_for_plan(plan, None)))
            .unwrap_or_else(|| {
                let project_root = manifest_path.parent().map(Path::to_path_buf).unwrap_or_default();
                vec![project_root.join(&manifest.project.root)]
            });
        Some(Self {
            project_manifest_path: manifest_path,
            workspace_manifest_path,
            project_kind,
            compile_plan,
            module_roots,
        })
    }

    /// Legacy hook retained until document/LSP callers migrate to query-backed assembly (W3-B/W3-D).
    ///
    /// Always returns `None`; assembly is no longer built or cached on the session handle.
    pub fn assembly_for_entry(&mut self, _entry_path: &Path, _entry_source: &str) -> Option<&ProgramAssembly> {
        None
    }

    /// Whether compiler-mod contract items may be placed at module scope in this project's Beskid sources.
    #[inline]
    pub fn module_level_meta_items_allowed(&self) -> bool {
        self.project_kind == ProjectKind::Mod
    }
}

/// On-disk source roots for the primary project plus each dependency in `plan` (module path existence checks).
pub fn module_roots_for_plan(plan: &CompilePlan) -> Vec<PathBuf> {
    module_roots_from_effective(&effective_roots_for_plan(plan, None))
}
