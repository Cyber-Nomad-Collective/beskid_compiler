//! `*.bproj` / `*.bws` manifests, lockfiles, dependency graphs, compile plans, and validation.

pub mod assembly;
pub mod compile_plan;
pub mod discovery;
pub mod error;
pub mod graph;
pub mod manifest_resolve;
pub mod model;
pub mod parser;
mod readme;
pub mod validator;
pub mod workflow;

pub use assembly::{
    AssemblyError, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    UnitHir, UnitMaterializer, assemble_program, assemble_program_with_materializer,
    assembly_options_for_plan, build_hir_units, effective_roots_for_plan,
    effective_roots_from_lockfile, effective_roots_from_plan_and_workspace,
    infer_logical_module_path, module_path_exists_on_disk, module_path_to_relative_path,
    module_roots_from_effective, resolve_module_file,
};
pub use compile_plan::{
    build_compile_plan, build_compile_plan_with_policy, build_compile_plan_with_policy_and_graph,
    load_manifest_from_path, plan_entry_path,
};
pub use discovery::{
    LEGACY_PROJECT_FILE_NAME, LEGACY_WORKSPACE_FILE_NAME, PROJECT_MANIFEST_EXTENSION,
    WORKSPACE_MANIFEST_EXTENSION, discover_project_file, discover_project_manifest_in_dir,
    discover_workspace_file, discover_workspace_manifest_in_dir, is_project_manifest_path,
    is_workspace_manifest_path, project_manifest_for_member_dir, reject_legacy_manifest_path,
};
pub use error::ProjectError;
pub use graph::{
    DependencyEdge, ProjectGraph, ProjectGraphBuildOptions, ProjectGraphNode, UnresolvedDependency,
    UnresolvedDependencyKind, WorkspaceResolutionRules, build_project_graph,
    build_project_graph_with_options, collect_dependency_projects, collect_unresolved_dependencies,
    discover_workspace_resolution_rules,
};
pub use manifest_resolve::{
    discover_project_manifest_from_input_or_cwd, resolve_project_manifest_for_cwd,
    resolve_project_manifest_for_source_path, resolve_project_manifest_from_workspace,
    resolve_workspace_candidate_path, resolve_workspace_candidate_with_summary,
};
pub use model::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, Dependency, DependencySource,
    MaterializedDependencyProject, PreparedProjectWorkspace, ProjectKind, ProjectLinkSection,
    ProjectManifest, ProjectModSection, ProjectSection, ProjectTemplateSection,
    ResolvedDependencyProject, Target, TargetKind, UnresolvedDependencyNote,
    UnresolvedDependencyPolicy, WorkspaceManifest, WorkspaceMember, WorkspaceOverride,
    WorkspaceRegistry, WorkspaceResolutionSummary, WorkspaceSection,
};
pub use parser::{parse_manifest, parse_workspace_manifest};
pub use readme::{
    PACKAGE_README_ARTIFACT_NAME, discover_readme_for_package_root, is_package_root_readme_entry,
    resolve_readme_file_path,
};
pub use validator::{MOD_CAPABILITY_NAMES, validate_manifest, validate_workspace_manifest};
pub use workflow::{
    PROJECT_LOCK_FILE_NAME, ProjectLockDependencyEntry, WorkspacePrepareOptions,
    load_project_lock_dependencies, prepare_project_workspace,
    prepare_project_workspace_with_options,
};
