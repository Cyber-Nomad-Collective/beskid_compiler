use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub project: ProjectSection,
    pub targets: Vec<Target>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceManifest {
    pub workspace: WorkspaceSection,
    pub members: Vec<WorkspaceMember>,
    pub overrides: Vec<WorkspaceOverride>,
    pub registries: Vec<WorkspaceRegistry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSection {
    pub name: String,
    pub resolver: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMember {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOverride {
    pub dependency: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRegistry {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub root: String,
    pub root_namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub kind: TargetKind,
    pub entry: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    App,
    Lib,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub source: DependencySource,
    pub path: Option<String>,
    pub url: Option<String>,
    pub rev: Option<String>,
    pub version: Option<String>,
    pub registry: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    Path,
    Git,
    Registry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedDependencyPolicy {
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedDependencyNote {
    pub dependency_name: String,
    pub source: DependencySource,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilePlan {
    pub project_root: PathBuf,
    pub manifest_path: PathBuf,
    pub project_name: String,
    pub source_root: PathBuf,
    pub target: Target,
    pub dependency_projects: Vec<ResolvedDependencyProject>,
    pub unresolved_dependencies: Vec<UnresolvedDependencyNote>,
    pub has_std_dependency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDependencyProject {
    pub dependency_name: String,
    pub manifest_path: PathBuf,
    pub project_root: PathBuf,
    pub project_name: String,
    pub source_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedDependencyProject {
    pub dependency_name: String,
    pub manifest_path: PathBuf,
    pub project_name: String,
    pub materialized_project_root: PathBuf,
    pub materialized_source_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedProjectWorkspace {
    pub lockfile_path: PathBuf,
    pub materialized_project_root: PathBuf,
    pub materialized_source_root: PathBuf,
    pub materialized_dependencies: Vec<MaterializedDependencyProject>,
}
