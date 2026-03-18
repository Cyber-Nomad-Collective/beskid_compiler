use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::projects::discovery::discover_workspace_file;
use crate::projects::error::ProjectError;
use crate::projects::graph::loader::load_manifest_from_path;
use crate::projects::graph::pathing::{normalize_existing_path, project_root_from_manifest_path};
use crate::projects::graph::project_graph::{ProjectGraph, ProjectGraphNode};
use crate::projects::graph::resolver::{WorkspaceResolutionRules, resolve_dependencies};
use crate::projects::parser::parse_workspace_manifest;
use crate::projects::validator::validate_workspace_manifest;

pub fn build_project_graph(manifest_path: &Path) -> Result<ProjectGraph, ProjectError> {
    let root_manifest_path = normalize_existing_path(manifest_path);
    let root_project_root = project_root_from_manifest_path(&root_manifest_path)?;
    let root_manifest = load_manifest_from_path(&root_manifest_path)?;

    let mut dag = daggy::Dag::new();
    let root = dag.add_node(ProjectGraphNode::RootProject {
        manifest_path: root_manifest_path.clone(),
        project_root: root_project_root.clone(),
        project_name: root_manifest.project.name.clone(),
        source_root: root_project_root.join(&root_manifest.project.root),
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

fn discover_workspace_resolution_rules(
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
    let registries = workspace_manifest
        .registries
        .into_iter()
        .map(|item| item.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    Ok(Some(WorkspaceResolutionRules::new(overrides, registries)))
}
