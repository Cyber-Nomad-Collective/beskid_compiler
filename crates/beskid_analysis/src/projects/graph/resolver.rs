use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

const ENV_CORELIB_ROOT: &str = "BESKID_CORELIB_ROOT";

use daggy::{Dag, NodeIndex};

use crate::projects::error::ProjectError;
use crate::projects::graph::loader::load_manifest_from_path;
use crate::projects::discovery::discover_project_manifest_in_dir;
use crate::projects::graph::pathing::{
    dependency_manifest_path, normalize_existing_path, project_root_from_manifest_path,
};
use crate::projects::graph::project_graph::{DependencyEdge, ProjectGraphNode};
use crate::projects::model::{DependencySource, ProjectKind, ProjectManifest, WorkspaceMember};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceResolutionRules {
    /// Parent directory of `Workspace.proj` when a workspace was discovered.
    pub workspace_root: Option<PathBuf>,
    /// `member` entries from `Workspace.proj`.
    pub workspace_members: Vec<WorkspaceMember>,
    overrides_by_dependency: HashMap<String, String>,
    registry_aliases: HashSet<String>,
    /// Lowercased registry alias -> base URL (no trailing slash), from `Workspace.proj`.
    registry_urls: HashMap<String, String>,
}

impl Default for WorkspaceResolutionRules {
    fn default() -> Self {
        Self::new(
            None,
            Vec::new(),
            HashMap::new(),
            HashSet::new(),
            HashMap::new(),
        )
    }
}

impl WorkspaceResolutionRules {
    pub fn new(
        workspace_root: Option<PathBuf>,
        workspace_members: Vec<WorkspaceMember>,
        overrides_by_dependency: HashMap<String, String>,
        registry_aliases: HashSet<String>,
        registry_urls: HashMap<String, String>,
    ) -> Self {
        Self {
            workspace_root,
            workspace_members,
            overrides_by_dependency,
            registry_aliases,
            registry_urls,
        }
    }

    fn override_version_for(&self, dependency_name: &str) -> Option<&str> {
        self.overrides_by_dependency
            .get(&dependency_name.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn has_registry_alias(&self, alias: &str) -> bool {
        self.registry_aliases.contains(&alias.to_ascii_lowercase())
    }

    /// Base URL for a registry dependency (`alias` from the lock descriptor), or `default` entry.
    pub fn registry_base_url(&self, registry_alias: Option<&str>) -> Option<&str> {
        if let Some(alias) = registry_alias {
            let key = alias.to_ascii_lowercase();
            if let Some(url) = self.registry_urls.get(&key) {
                return Some(url.as_str());
            }
        }
        self.registry_urls.get("default").map(String::as_str)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_dependencies(
    dag: &mut Dag<ProjectGraphNode, DependencyEdge>,
    consumer_index: NodeIndex,
    consumer_manifest_path: &Path,
    consumer_manifest: &ProjectManifest,
    workspace_rules: Option<&WorkspaceResolutionRules>,
    node_by_manifest: &mut HashMap<PathBuf, NodeIndex>,
    visiting: &mut Vec<PathBuf>,
    has_std_dependency: &mut bool,
) -> Result<(), ProjectError> {
    let consumer_project_root = project_root_from_manifest_path(consumer_manifest_path)?;
    let has_explicit_std_dependency = consumer_manifest
        .dependencies
        .iter()
        .any(|dependency| dependency.name.eq_ignore_ascii_case("Std"));
    let is_std_project = consumer_manifest.project.name.eq_ignore_ascii_case("Std")
        || is_std_manifest_path(consumer_manifest_path);

    for dependency in &consumer_manifest.dependencies {
        match dependency.source {
            DependencySource::Path => {
                let fallback_std_path = if dependency.name.eq_ignore_ascii_case("Std") {
                    default_corelib_dependency_path()
                } else {
                    None
                };

                let relative_path = dependency
                    .path
                    .as_deref()
                    .or(fallback_std_path.as_deref())
                    .ok_or_else(|| {
                        ProjectError::Validation(format!(
                            "dependency `{}` with source=\"path\" requires `path`",
                            dependency.name
                        ))
                    })?;

                attach_path_dependency(
                    dag,
                    consumer_index,
                    consumer_manifest_path,
                    &consumer_project_root,
                    &dependency.name,
                    relative_path,
                    workspace_rules,
                    node_by_manifest,
                    visiting,
                    has_std_dependency,
                )?;
            }
            DependencySource::Git => {
                let url = dependency.url.clone().ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "dependency `{}` with source=\"git\" requires `url`",
                        dependency.name
                    ))
                })?;
                let rev = dependency.rev.clone().ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "dependency `{}` with source=\"git\" requires `rev`",
                        dependency.name
                    ))
                })?;

                let unresolved_index = dag.add_node(ProjectGraphNode::UnresolvedGitDependency {
                    dependency_name: dependency.name.clone(),
                    url,
                    rev,
                });

                if dag
                    .add_edge(
                        consumer_index,
                        unresolved_index,
                        DependencyEdge {
                            dependency_name: dependency.name.clone(),
                            source: dependency.source,
                        },
                    )
                    .is_err()
                {
                    return Err(ProjectError::DependencyCycle(format!(
                        "{} -> external:{} -> {}",
                        consumer_manifest_path.display(),
                        dependency.name,
                        consumer_manifest_path.display()
                    )));
                }
            }
            DependencySource::Registry => {
                let mut version = dependency.version.clone().ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "dependency `{}` with source=\"registry\" requires `version`",
                        dependency.name
                    ))
                })?;

                if let Some(rules) = workspace_rules {
                    if let Some(override_version) = rules.override_version_for(&dependency.name) {
                        version = override_version.to_string();
                    }

                    if let Some(registry_alias) = dependency.registry.as_deref()
                        && !rules.has_registry_alias(registry_alias)
                    {
                        return Err(ProjectError::Validation(format!(
                            "dependency `{}` references unknown workspace registry alias `{}`",
                            dependency.name, registry_alias
                        )));
                    }
                }

                let unresolved_index =
                    dag.add_node(ProjectGraphNode::UnresolvedRegistryDependency {
                        dependency_name: dependency.name.clone(),
                        version,
                        registry: dependency.registry.clone(),
                    });

                if dag
                    .add_edge(
                        consumer_index,
                        unresolved_index,
                        DependencyEdge {
                            dependency_name: dependency.name.clone(),
                            source: dependency.source,
                        },
                    )
                    .is_err()
                {
                    return Err(ProjectError::DependencyCycle(format!(
                        "{} -> external:{} -> {}",
                        consumer_manifest_path.display(),
                        dependency.name,
                        consumer_manifest_path.display()
                    )));
                }
            }
        }
    }

    if !has_explicit_std_dependency
        && !is_std_project
        && consumer_manifest.project.kind != ProjectKind::Template
        && !is_corelib_workspace_shard_manifest(consumer_manifest_path)
        && !depends_on_corelib_aggregate(consumer_manifest, &consumer_project_root)
        && let Some(corelib_path) = default_corelib_dependency_path()
    {
        attach_path_dependency(
            dag,
            consumer_index,
            consumer_manifest_path,
            &consumer_project_root,
            "Std",
            &corelib_path,
            workspace_rules,
            node_by_manifest,
            visiting,
            has_std_dependency,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn attach_path_dependency(
    dag: &mut Dag<ProjectGraphNode, DependencyEdge>,
    consumer_index: NodeIndex,
    consumer_manifest_path: &Path,
    consumer_project_root: &Path,
    dependency_name: &str,
    relative_path: &str,
    workspace_rules: Option<&WorkspaceResolutionRules>,
    node_by_manifest: &mut HashMap<PathBuf, NodeIndex>,
    visiting: &mut Vec<PathBuf>,
    has_std_dependency: &mut bool,
) -> Result<(), ProjectError> {
    let dependency_manifest_path = dependency_manifest_path(consumer_project_root, relative_path)?;

    if let Some(cycle_start) = visiting
        .iter()
        .position(|path| path == &dependency_manifest_path)
    {
        return Err(ProjectError::DependencyCycle(format_cycle_from_visiting(
            visiting,
            cycle_start,
            &dependency_manifest_path,
        )));
    }

    let dependency_index = if let Some(existing_index) =
        node_by_manifest.get(&dependency_manifest_path)
    {
        *existing_index
    } else {
        let dependency_manifest = load_manifest_from_path(&dependency_manifest_path)?;
        let dependency_project_root = project_root_from_manifest_path(&dependency_manifest_path)?;
        let dependency_source_root =
            dependency_project_root.join(&dependency_manifest.project.root);

        let dependency_index = dag.add_node(ProjectGraphNode::ResolvedPathDependency {
            dependency_name: dependency_name.to_string(),
            manifest_path: dependency_manifest_path.clone(),
            project_root: dependency_project_root,
            project_name: dependency_manifest.project.name.clone(),
            source_root: dependency_source_root,
            project_kind: dependency_manifest.project.kind,
        });

        node_by_manifest.insert(dependency_manifest_path.clone(), dependency_index);

        visiting.push(dependency_manifest_path.clone());
        resolve_dependencies(
            dag,
            dependency_index,
            &dependency_manifest_path,
            &dependency_manifest,
            workspace_rules,
            node_by_manifest,
            visiting,
            has_std_dependency,
        )?;
        visiting.pop();

        dependency_index
    };

    if dag
        .add_edge(
            consumer_index,
            dependency_index,
            DependencyEdge {
                dependency_name: dependency_name.to_string(),
                source: DependencySource::Path,
            },
        )
        .is_err()
    {
        return Err(ProjectError::DependencyCycle(format!(
            "{} -> {} -> {}",
            consumer_manifest_path.display(),
            dependency_manifest_path.display(),
            consumer_manifest_path.display()
        )));
    }

    if dependency_name.eq_ignore_ascii_case("Std") {
        *has_std_dependency = true;
    }

    Ok(())
}

fn default_corelib_dependency_path() -> Option<String> {
    if let Ok(explicit_root) = env::var(ENV_CORELIB_ROOT) {
        let root = PathBuf::from(explicit_root);
        return Some(corelib_aggregate_project_dir(&root).display().to_string());
    }

    discover_repo_corelib_root().map(|path| path.display().to_string())
}

/// `BESKID_CORELIB_ROOT` / install roots may be either the aggregate `beskid_corelib/` package
/// (contains `Project.proj`) or the parent **workspace** directory (has `Workspace.proj` and
/// nests `beskid_corelib/corelib.bproj`). `Std` path resolution must always end at the package.
fn corelib_aggregate_project_dir(root: &Path) -> PathBuf {
    let nested = root.join("beskid_corelib");
    if discover_project_manifest_in_dir(&nested)
        .ok()
        .flatten()
        .is_some()
    {
        nested
    } else if discover_project_manifest_in_dir(root)
        .ok()
        .flatten()
        .is_some()
    {
        root.to_path_buf()
    } else {
        nested
    }
}

/// True when the consumer already path-depends on the aggregate `beskid_corelib` project
/// (explicit `corelib` / `Std` link). Skip implicit `Std` injection so module paths stay on the
/// shard layout (`Testing::Assertions`) instead of duplicating the aggregate as `Std::*`.
fn depends_on_corelib_aggregate(
    consumer_manifest: &ProjectManifest,
    consumer_project_root: &Path,
) -> bool {
    let Some(corelib_root) = default_corelib_dependency_path().map(PathBuf::from) else {
        return false;
    };
    let corelib_manifest = discover_project_manifest_in_dir(&corelib_root)
        .ok()
        .flatten()
        .map(|path| normalize_existing_path(&path))
        .unwrap_or_else(|| normalize_existing_path(&corelib_root.join("corelib.bproj")));
    consumer_manifest.dependencies.iter().any(|dependency| {
        if dependency.source != DependencySource::Path {
            return false;
        }
        dependency.name.eq_ignore_ascii_case("corelib")
            || dependency.name.eq_ignore_ascii_case("Std")
            || dependency.path.as_ref().is_some_and(|relative_path| {
                dependency_manifest_path(consumer_project_root, relative_path)
                    .ok()
                    .map(|dependency_manifest| {
                        normalize_existing_path(&dependency_manifest) == corelib_manifest
                    })
                    .unwrap_or(false)
            })
    })
}

/// `Project.proj` files under `compiler/corelib/packages/*` are split shards of the aggregate
/// `corelib` package; they must not receive the implicit `Std` back-link to `beskid_corelib`
/// (that would create `beskid_corelib -> shard -> beskid_corelib` dependency cycles).
fn is_corelib_workspace_shard_manifest(manifest_path: &Path) -> bool {
    let Some(aggregate_root) = default_corelib_dependency_path().map(PathBuf::from) else {
        return false;
    };
    let aggregate_root = normalize_existing_path(&aggregate_root);
    let Some(workspace_root) = aggregate_root.parent().map(normalize_existing_path) else {
        return false;
    };
    let packages_root = normalize_existing_path(&workspace_root.join("packages"));
    let normalized_manifest = normalize_existing_path(manifest_path);
    normalized_manifest.starts_with(&packages_root)
}

fn discover_repo_corelib_root() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    for ancestor in cwd.ancestors() {
        let candidate = ancestor.join("corelib").join("beskid_corelib");
        if discover_project_manifest_in_dir(&candidate)
            .ok()
            .flatten()
            .is_some()
        {
            return Some(candidate);
        }
    }
    None
}

fn is_std_manifest_path(manifest_path: &Path) -> bool {
    let normalized_manifest = normalize_existing_path(manifest_path);
    let Some(corelib_root) = default_corelib_dependency_path() else {
        return false;
    };
    let corelib_manifest = discover_project_manifest_in_dir(&PathBuf::from(&corelib_root))
        .ok()
        .flatten()
        .map(|path| normalize_existing_path(&path))
        .unwrap_or_else(|| {
            normalize_existing_path(&PathBuf::from(corelib_root).join("corelib.bproj"))
        });
    normalized_manifest == corelib_manifest
}

fn format_cycle_from_visiting(
    visiting: &[PathBuf],
    cycle_start: usize,
    repeated_path: &Path,
) -> String {
    let mut cycle_chain = visiting[cycle_start..]
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    cycle_chain.push(repeated_path.display().to_string());
    cycle_chain.join(" -> ")
}
