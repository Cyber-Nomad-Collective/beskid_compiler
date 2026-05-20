//! Resolved project graph slice used for analysis and tooling (module roots, compile plan).

use std::path::{Path, PathBuf};

use crate::projects::{
    CompilePlan, ProjectGraphBuildOptions, ProjectKind, UnresolvedDependencyPolicy,
    build_compile_plan_with_policy_and_graph, discover_workspace_file, load_manifest_from_path,
    resolve_project_manifest_for_source_path,
};

/// Workspace-aware compilation slice: selected `Project.proj`, optional [`CompilePlan`] for
/// host and mod roots, [`ProjectKind`] for staged rules (for example compiler-mod contract placement),
/// and source roots for on-disk module path checks.
#[derive(Debug, Clone)]
pub struct CompilationContext {
    pub project_manifest_path: PathBuf,
    pub workspace_manifest_path: Option<PathBuf>,
    pub project_kind: ProjectKind,
    pub compile_plan: Option<CompilePlan>,
    pub module_roots: Vec<PathBuf>,
}

impl CompilationContext {
    /// Build context for IDE/analysis: no materialized dependency workspace required.
    pub fn try_for_analysis_path(path: &Path, workspace_member: Option<&str>) -> Option<Self> {
        Self::try_for_analysis_path_with_graph_options(
            path,
            workspace_member,
            ProjectGraphBuildOptions::default(),
        )
    }

    /// Like [`Self::try_for_analysis_path`], but forwards [`ProjectGraphBuildOptions`] (for example
    /// `workspace_member_for_meta_default`) into graph / compile-plan construction (legacy field; ignored).
    pub fn try_for_analysis_path_with_graph_options(
        path: &Path,
        workspace_member: Option<&str>,
        graph_options: ProjectGraphBuildOptions,
    ) -> Option<Self> {
        let (manifest_path, workspace_resolution) =
            resolve_project_manifest_for_source_path(path, workspace_member).ok()??;
        let manifest = load_manifest_from_path(&manifest_path).ok()?;
        let project_kind = manifest.project.kind;
        // Reuse workspace path from manifest resolution when present to avoid a second walk.
        let workspace_manifest_path = workspace_resolution
            .map(|summary| summary.workspace_manifest_path)
            .or_else(|| discover_workspace_file(&manifest_path));
        let compile_plan = match project_kind {
            ProjectKind::Host | ProjectKind::Mod => build_compile_plan_with_policy_and_graph(
                &manifest_path,
                None,
                UnresolvedDependencyPolicy::Error,
                graph_options,
            )
            .ok(),
        };
        let module_roots = compile_plan
            .as_ref()
            .map(crate::module_roots_for_plan)
            .unwrap_or_else(|| {
                let project_root = manifest_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
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

    /// Whether compiler-mod contract items may be placed at module scope in this project's Beskid sources.
    #[inline]
    pub fn module_level_meta_items_allowed(&self) -> bool {
        self.project_kind == ProjectKind::Mod
    }
}

/// On-disk source roots for the primary project plus each dependency in `plan` (module path existence checks).
pub fn module_roots_for_plan(plan: &CompilePlan) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(1 + plan.dependency_projects.len());
    roots.push(plan.source_root.clone());
    roots.extend(
        plan.dependency_projects
            .iter()
            .map(|dep| dep.source_root.clone()),
    );
    roots
}
