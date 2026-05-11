//! `Project.proj` / workspace manifests, lockfiles, dependency graphs, compile plans, and validation.

pub mod compile_plan;
pub mod discovery;
pub mod error;
pub mod graph;
pub mod manifest_resolve;
pub mod model;
pub mod parser;
pub mod validator;
pub mod workflow;

pub use compile_plan::{
    build_compile_plan, build_compile_plan_with_policy, build_compile_plan_with_policy_and_graph,
    load_manifest_from_path,
};
pub use discovery::{
    PROJECT_FILE_NAME, WORKSPACE_FILE_NAME, discover_project_file, discover_workspace_file,
};
pub use error::ProjectError;
pub use graph::{
    DependencyEdge, MetaAttachmentResolution, ProjectGraph, ProjectGraphBuildOptions,
    ProjectGraphNode, UnresolvedDependency, UnresolvedDependencyKind, WorkspaceResolutionRules,
    build_project_graph, build_project_graph_with_options, collect_dependency_projects,
    collect_unresolved_dependencies, discover_workspace_resolution_rules,
};
pub use manifest_resolve::{
    discover_project_manifest_from_input_or_cwd, resolve_project_manifest_for_cwd,
    resolve_project_manifest_for_source_path, resolve_project_manifest_from_workspace,
    resolve_workspace_candidate_path, resolve_workspace_candidate_with_summary,
};
pub use model::{
    AttachToSelector, CompilePlan, Dependency, DependencySource, MaterializedDependencyProject,
    PreparedProjectWorkspace, ProjectKind, ProjectManifest, ProjectMetaSection, ProjectSection,
    ResolvedDependencyProject, Target, TargetKind, UnresolvedDependencyNote,
    UnresolvedDependencyPolicy, WorkspaceManifest, WorkspaceMember, WorkspaceOverride,
    WorkspaceRegistry, WorkspaceResolutionSummary, WorkspaceSection,
};
pub use parser::{parse_manifest, parse_workspace_manifest};
pub use validator::{META_CAPABILITY_NAMES, validate_manifest, validate_workspace_manifest};
pub use workflow::{
    PROJECT_LOCK_FILE_NAME, ProjectLockDependencyEntry, WorkspacePrepareOptions,
    prepare_project_workspace, prepare_project_workspace_with_options,
};
