use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::projects::discovery::discover_workspace_file;
use crate::projects::error::ProjectError;
use crate::projects::graph::loader::load_manifest_from_path;
use crate::projects::graph::pathing::{normalize_existing_path, project_root_from_manifest_path};
use crate::projects::graph::project_graph::{ProjectGraph, ProjectGraphNode};
use crate::projects::graph::resolver::{WorkspaceResolutionRules, resolve_dependencies};
use crate::projects::parser::parse_workspace_manifest;
use crate::projects::validator::validate_workspace_manifest;

pub fn build_project_graph(manifest_path: &Path) -> Result<ProjectGraph, ProjectError> {
    build_project_graph_with_options(manifest_path, ProjectGraphBuildOptions::default())
}

/// Load workspace policy (overrides, registry aliases and URLs) for a root `Project.proj`.
pub fn discover_workspace_resolution_rules(
    root_manifest_path: &Path,
) -> Result<Option<WorkspaceResolutionRules>, ProjectError> {
    let Some(workspace_manifest_path) = discover_workspace_file(root_manifest_path) else {
        return Ok(None);
    };

    let workspace_source = std::fs::read_to_string(&workspace_manifest_path).map_err(|source| {
        ProjectError::ReadManifest {
            path: workspace_manifest_path.clone(),
            source,
        }
    })?;

    let workspace_manifest = parse_workspace_manifest(&workspace_source)?;
    validate_workspace_manifest(&workspace_manifest)?;

    let overrides = workspace_manifest
        .overrides
        .into_iter()
        .map(|item| (item.dependency.to_ascii_lowercase(), item.version))
        .collect::<HashMap<_, _>>();

    let mut registry_aliases = HashSet::new();
    let mut registry_urls = HashMap::new();
    for item in workspace_manifest.registries {
        let name = item.name.to_ascii_lowercase();
        registry_aliases.insert(name.clone());
        let url = item.url.trim().trim_end_matches('/').to_string();
        if !url.is_empty() {
            registry_urls.insert(name, url);
        }
    }

    let workspace_root: Option<PathBuf> = workspace_manifest_path.parent().map(Path::to_path_buf);
    let workspace_members = workspace_manifest.members.clone();

    Ok(Some(WorkspaceResolutionRules::new(
        workspace_root,
        workspace_members,
        overrides,
        registry_aliases,
        registry_urls,
    )))
}

/// Build options for [`build_project_graph_with_options`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectGraphBuildOptions {
    /// Legacy field retained for callers that still pass workspace member hints; ignored for mod discovery.
    pub workspace_member_for_meta_default: Option<String>,
}

pub fn build_project_graph_with_options(
    manifest_path: &Path,
    _options: ProjectGraphBuildOptions,
) -> Result<ProjectGraph, ProjectError> {
    let root_manifest_path = normalize_existing_path(manifest_path);
    let root_project_root = project_root_from_manifest_path(&root_manifest_path)?;
    let root_manifest = load_manifest_from_path(&root_manifest_path)?;

    let mut dag = daggy::Dag::new();
    let root = dag.add_node(ProjectGraphNode::RootProject {
        manifest_path: root_manifest_path.clone(),
        project_root: root_project_root.clone(),
        project_name: root_manifest.project.name.clone(),
        source_root: root_project_root.join(&root_manifest.project.root),
        project_kind: root_manifest.project.kind,
    });

    let mut node_by_manifest = HashMap::new();
    node_by_manifest.insert(root_manifest_path.clone(), root);

    let mut visiting = vec![root_manifest_path.clone()];
    let mut has_std_dependency = false;
    let workspace_rules = discover_workspace_resolution_rules(&root_manifest_path)?;

    resolve_dependencies(
        &mut dag,
        root,
        &root_manifest_path,
        &root_manifest,
        workspace_rules.as_ref(),
        &mut node_by_manifest,
        &mut visiting,
        &mut has_std_dependency,
    )?;

    Ok(ProjectGraph {
        dag,
        root,
        root_manifest_path,
        root_project_root,
        root_manifest,
        node_by_manifest,
        has_std_dependency,
    })
}
