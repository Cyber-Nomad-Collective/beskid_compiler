//! Resolved project graph slice used for analysis and tooling (module roots, compile plan).

use std::path::{Path, PathBuf};

use crate::projects::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, PreparedProjectWorkspace, ProgramAssembly,
    PROJECT_LOCK_FILE_NAME, ProjectGraphBuildOptions, ProjectKind, UnresolvedDependencyPolicy,
    WorkspacePrepareOptions, assemble_program, build_compile_plan_with_policy_and_graph,
    discover_workspace_file, effective_roots_for_plan, load_manifest_from_path,
    module_roots_from_effective, prepare_project_workspace_with_options,
    resolve_project_manifest_for_source_path,
};

/// Workspace-aware compilation slice: selected `Project.proj`, optional [`CompilePlan`] for
/// host and mod roots, [`ProjectKind`] for staged rules (for example compiler-mod contract placement),
/// materialized-first [`module_roots`](Self::module_roots), and optional cached [`ProgramAssembly`].
#[derive(Debug, Clone)]
pub struct CompilationContext {
    pub project_manifest_path: PathBuf,
    pub workspace_manifest_path: Option<PathBuf>,
    pub project_kind: ProjectKind,
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
    pub module_roots: Vec<PathBuf>,
    pub assembly: Option<ProgramAssembly>,
}

impl CompilationContext {
    /// Build context for IDE/analysis: uses lockfile materialized paths when present; no full materialize.
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
        let workspace_manifest_path = workspace_resolution
            .map(|summary| summary.workspace_manifest_path)
            .or_else(|| discover_workspace_file(&manifest_path));
        let compile_plan = match project_kind {
            ProjectKind::Template => None,
            ProjectKind::Host | ProjectKind::Mod => build_compile_plan_with_policy_and_graph(
                &manifest_path,
                None,
                UnresolvedDependencyPolicy::Error,
                graph_options,
            )
            .ok(),
        };
        let prepared_workspace = None;
        let module_roots = compile_plan
            .as_ref()
            .map(|plan| {
                module_roots_from_effective(&effective_roots_for_plan(
                    plan,
                    prepared_workspace.as_ref(),
                ))
            })
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
            prepared_workspace,
            module_roots,
            assembly: None,
        })
    }

    /// Lazily build or return cached assembly for `entry_path` (import-closure for IDE).
    pub fn assembly_for_entry(
        &mut self,
        entry_path: &Path,
        entry_source: &str,
    ) -> Option<&ProgramAssembly> {
        if self.assembly.is_some() {
            return self.assembly.as_ref();
        }
        let plan = self.compile_plan.as_ref()?;

        // Std shard modules (e.g. System/Output) resolve from materialized dependency roots.
        // IDE context starts without a prepared workspace; materialize lazily on first assembly.
        if self.prepared_workspace.is_none() {
            let lockfile = plan.manifest_path.with_file_name(PROJECT_LOCK_FILE_NAME);
            let prepare_options = WorkspacePrepareOptions {
                frozen: false,
                locked: lockfile.is_file(),
            };
            if let Ok(workspace) =
                prepare_project_workspace_with_options(plan, prepare_options)
            {
                self.prepared_workspace = Some(workspace);
                self.module_roots = module_roots_from_effective(&effective_roots_for_plan(
                    plan,
                    self.prepared_workspace.as_ref(),
                ));
            }
        }

        let mut options = AssemblyOptions::default();
        options.discovery = AssemblyDiscovery::ImportClosure;
        if entry_path
            .file_name()
            .is_some_and(|name| name == "Prelude.bd")
        {
            options.include_std_prelude = false;
        }
        self.assembly = assemble_program(
            plan,
            self.prepared_workspace.as_ref(),
            entry_path,
            Some(entry_source),
            &options,
        )
        .ok();
        self.assembly.as_ref()
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
