use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use daggy::{Dag, NodeIndex};

use crate::projects::error::ProjectError;
use crate::projects::graph::loader::load_manifest_from_path;
use crate::projects::graph::pathing::{dependency_manifest_path, project_root_from_manifest_path};
use crate::projects::graph::project_graph::{DependencyEdge, ProjectGraphNode};
use crate::projects::model::{DependencySource, ProjectManifest};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceResolutionRules {
    overrides_by_dependency: HashMap<String, String>,
    registry_aliases: HashSet<String>,
}

impl WorkspaceResolutionRules {
    pub fn new(
        overrides_by_dependency: HashMap<String, String>,
        registry_aliases: HashSet<String>,
    ) -> Self {
        Self {
            overrides_by_dependency,
            registry_aliases,
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

    for dependency in &consumer_manifest.dependencies {
        match dependency.source {
            DependencySource::Path => {
                let relative_path = dependency.path.as_deref().ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "dependency `{}` with source=\"path\" requires `path`",
                        dependency.name
                    ))
                })?;

                let dependency_manifest_path =
                    dependency_manifest_path(&consumer_project_root, relative_path);

                if !dependency_manifest_path.is_file() {
                    return Err(ProjectError::DependencyManifestNotFound {
                        dependency: dependency.name.clone(),
                        path: dependency_manifest_path,
                    });
                }

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
                    let dependency_project_root =
                        project_root_from_manifest_path(&dependency_manifest_path)?;
                    let dependency_source_root =
                        dependency_project_root.join(&dependency_manifest.project.root);

                    let dependency_index = dag.add_node(ProjectGraphNode::ResolvedPathDependency {
                        dependency_name: dependency.name.clone(),
                        manifest_path: dependency_manifest_path.clone(),
                        project_root: dependency_project_root,
                        project_name: dependency_manifest.project.name.clone(),
                        source_root: dependency_source_root,
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
                            dependency_name: dependency.name.clone(),
                            source: dependency.source,
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

                if dependency.name.eq_ignore_ascii_case("Std") {
                    *has_std_dependency = true;
                }
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

                    if let Some(registry_alias) = dependency.registry.as_deref() {
                        if !rules.has_registry_alias(registry_alias) {
                            return Err(ProjectError::Validation(format!(
                                "dependency `{}` references unknown workspace registry alias `{}`",
                                dependency.name, registry_alias
                            )));
                        }
                    }
                }

                let unresolved_index = dag.add_node(ProjectGraphNode::UnresolvedRegistryDependency {
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

    Ok(())
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
