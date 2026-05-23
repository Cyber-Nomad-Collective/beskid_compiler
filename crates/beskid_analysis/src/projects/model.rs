use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub project: ProjectSection,
    pub targets: Vec<Target>,
    pub dependencies: Vec<Dependency>,
    pub link: Option<ProjectLinkSection>,
}

/// Top-level `link { ... }` block populating foreign linker inputs (v0.3).
///
/// Maps the **Foreign library import** platform-spec feature; see
/// `site/website/src/content/docs/platform-spec/tooling/manifests-and-lockfiles/project-manifest-contract/project-link-libraries.mdx`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectLinkSection {
    /// Logical library names matching `Extern` `Library` strings (for example `"libc"`).
    pub libraries: Vec<String>,
    /// Additional linker search paths (passed as `-L`/`/LIBPATH:`).
    pub search_paths: Vec<String>,
    /// Raw linker arguments appended after provider resolution.
    pub extra_args: Vec<String>,
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

/// `project.type` in `Project.proj` (`Host` is the implicit default when omitted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectKind {
    #[default]
    Host,
    Mod,
    Template,
}

/// Nested `project.template { ... }` block for [`ProjectKind::Template`] manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTemplateSection {
    pub short_name: Option<String>,
    pub identity: Option<String>,
}

/// Nested `project.mod { ... }` block for [`ProjectKind::Mod`] manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectModSection {
    pub max_generator_rounds: Option<u32>,
    pub capabilities: Option<Vec<String>>,
    pub artifact_policy: Option<String>,
}

impl ProjectModSection {
    /// Normative default when `maxGeneratorRounds` is omitted (project manifest contract).
    pub fn resolved_max_generator_rounds(&self) -> u32 {
        self.max_generator_rounds.unwrap_or(4)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub root: String,
    pub root_namespace: Option<String>,
    pub kind: ProjectKind,
    pub mod_section: Option<ProjectModSection>,
    pub template_section: Option<ProjectTemplateSection>,
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

/// Workspace member selection aligned with [`super::manifest_resolve`](crate::projects::manifest_resolve).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceResolutionSummary {
    pub workspace_manifest_path: PathBuf,
    pub selected_member_id: String,
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

/// How assembly discovers `.bd` files under effective roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyDiscovery {
    /// Entry plus transitive `use` paths and optional std prelude.
    ImportClosure,
    /// All `*.bd` files under each root (IDE / project analyze), capped by `max_units`.
    WorkspaceScan,
}

/// Options for [`super::assembly::assemble_program`].
#[derive(Debug, Clone)]
pub struct AssemblyOptions {
    pub discovery: AssemblyDiscovery,
    pub include_std_prelude: bool,
    pub max_units: usize,
    /// When true, skip units that fail to parse instead of failing the whole assembly (`beskid doc`).
    pub skip_parse_errors: bool,
}

impl Default for AssemblyOptions {
    fn default() -> Self {
        Self {
            discovery: AssemblyDiscovery::ImportClosure,
            include_std_prelude: true,
            max_units: 4096,
            skip_parse_errors: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedProjectWorkspace {
    pub lockfile_path: PathBuf,
    pub materialized_project_root: PathBuf,
    pub materialized_source_root: PathBuf,
    pub materialized_dependencies: Vec<MaterializedDependencyProject>,
}
