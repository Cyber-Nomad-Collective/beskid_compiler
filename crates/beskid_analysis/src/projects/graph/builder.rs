use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use daggy::NodeIndex;

use crate::projects::discovery::discover_workspace_file;
use crate::projects::error::ProjectError;
use crate::projects::graph::loader::load_manifest_from_path;
use crate::projects::graph::pathing::{normalize_existing_path, project_root_from_manifest_path};
use crate::projects::graph::project_graph::{
    DependencyEdge, MetaAttachmentResolution, ProjectGraph, ProjectGraphNode,
};
use crate::projects::graph::resolver::{
    WorkspaceResolutionRules, resolve_dependencies, resolve_meta_attach_to_member_ids,
    workspace_member_project_kinds,
};
use crate::projects::model::ProjectKind;
use crate::projects::parser::parse_workspace_manifest;
use crate::projects::validator::{
    validate_meta_entry_modules_on_disk, validate_workspace_manifest,
};

pub fn build_project_graph(manifest_path: &Path) -> Result<ProjectGraph, ProjectError> {
    build_project_graph_with_options(manifest_path, ProjectGraphBuildOptions::default())
}

fn finalize_meta_attachments(
    dag: &mut daggy::Dag<ProjectGraphNode, DependencyEdge>,
    node_by_manifest: &HashMap<PathBuf, NodeIndex>,
    workspace_rules: Option<&WorkspaceResolutionRules>,
    options: &ProjectGraphBuildOptions,
) -> Result<(), ProjectError> {
    let member_kinds: HashMap<String, ProjectKind> = match workspace_rules {
        Some(rules) if rules.workspace_root.is_some() && !rules.workspace_members.is_empty() => {
            workspace_member_project_kinds(rules)?
        }
        _ => HashMap::new(),
    };

    for &idx in node_by_manifest.values() {
        let (manifest_path, source_root) = match dag.node_weight(idx) {
            Some(ProjectGraphNode::RootProject {
                manifest_path,
                source_root,
                ..
            }) => (manifest_path.clone(), source_root.clone()),
            Some(ProjectGraphNode::ResolvedPathDependency {
                manifest_path,
                source_root,
                ..
            }) => (manifest_path.clone(), source_root.clone()),
            _ => continue,
        };

        let manifest = load_manifest_from_path(&manifest_path)?;
        if manifest.project.kind != ProjectKind::Meta {
            continue;
        }

        validate_meta_entry_modules_on_disk(&manifest, &source_root)?;

        let Some(rules) = workspace_rules else {
            return Err(ProjectError::meta_contract(
                "E1811",
                "`Meta` projects require a sibling `Workspace.proj` to resolve `attachTo`",
            ));
        };
        if rules.workspace_root.is_none() || rules.workspace_members.is_empty() {
            return Err(ProjectError::meta_contract(
                "E1812",
                "`Meta` projects require a `Workspace.proj` with at least one `member` to resolve `attachTo`",
            ));
        }

        let Some(meta) = manifest.project.meta.as_ref() else {
            continue;
        };

        let ids = resolve_meta_attach_to_member_ids(
            meta,
            &member_kinds,
            &rules.workspace_members,
            options.workspace_member_for_meta_default.as_deref(),
        )?;

        match dag.node_weight_mut(idx) {
            Some(ProjectGraphNode::RootProject {
                meta_attachments, ..
            }) => {
                *meta_attachments = Some(MetaAttachmentResolution {
                    host_member_ids: ids,
                });
            }
            Some(ProjectGraphNode::ResolvedPathDependency {
                meta_attachments, ..
            }) => {
                *meta_attachments = Some(MetaAttachmentResolution {
                    host_member_ids: ids,
                });
            }
            _ => {}
        }
    }

    Ok(())
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
    /// Resolves `attachTo: default` for `Meta` projects (see workspace resolution contract).
    pub workspace_member_for_meta_default: Option<String>,
}

pub fn build_project_graph_with_options(
    manifest_path: &Path,
    options: ProjectGraphBuildOptions,
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
        meta_section: root_manifest.project.meta.clone(),
        meta_attachments: None,
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

    finalize_meta_attachments(
        &mut dag,
        &node_by_manifest,
        workspace_rules.as_ref(),
        &options,
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
