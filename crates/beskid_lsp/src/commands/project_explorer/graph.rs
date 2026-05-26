//! Project graph and dependency execute-command payloads.

use beskid_analysis::projects::{
    ProjectGraphBuildOptions, ProjectLockDependencyEntry, UnresolvedDependencyKind,
    build_project_graph_with_options, collect_unresolved_dependencies, load_project_lock_dependencies,
    parse_manifest, DependencySource,
};
use daggy::petgraph::visit::EdgeRef;
use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::Result;

use crate::protocol::execute_args::missing_args;
use crate::workspace_scan::path_to_uri_string;

pub(crate) fn get_project_graph(project_uri: &str) -> Result<Value> {
    let manifest_path = super::manifest_path_from_uri(project_uri)?;
    let graph = build_project_graph_with_options(&manifest_path, ProjectGraphBuildOptions::default())
        .map_err(|_| missing_args())?;

    let mut nodes = Vec::new();
    let mut node_id_by_index = std::collections::HashMap::new();

    for index in graph.dag.graph().node_indices() {
        let id = index.index().to_string();
        node_id_by_index.insert(index, id.clone());
        let Some(weight) = graph.dag.graph().node_weight(index) else {
            continue;
        };
        nodes.push(serialize_graph_node(weight, &id));
    }

    let mut edges = Vec::new();
    for edge in graph.dag.graph().edge_references() {
        let from = node_id_by_index.get(&edge.source()).cloned();
        let to = node_id_by_index.get(&edge.target()).cloned();
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        edges.push(json!({
            "from": from,
            "to": to,
            "dependencyName": edge.weight().dependency_name,
            "source": dependency_source_str(edge.weight().source),
        }));
    }

    let unresolved: Vec<Value> = collect_unresolved_dependencies(&graph)
        .into_iter()
        .map(serialize_unresolved_dependency)
        .collect();

    Ok(json!({
        "projectUri": project_uri,
        "nodes": nodes,
        "edges": edges,
        "unresolved": unresolved,
    }))
}

pub(crate) fn get_project_dependencies(project_uri: &str) -> Result<Value> {
    let manifest_path = super::manifest_path_from_uri(project_uri)?;
    let text = std::fs::read_to_string(&manifest_path).map_err(|_| missing_args())?;
    let manifest = parse_manifest(&text).map_err(|_| missing_args())?;

    let declared: Vec<Value> = manifest
        .dependencies
        .iter()
        .map(|dep| {
            json!({
                "name": dep.name,
                "source": dependency_source_str(dep.source),
                "path": dep.path,
                "url": dep.url,
                "rev": dep.rev,
                "version": dep.version,
                "registry": dep.registry,
            })
        })
        .collect();

    let project_root = manifest_path.parent().ok_or_else(missing_args)?;
    let locked = load_project_lock_dependencies(project_root)
        .unwrap_or_default()
        .iter()
        .map(serialize_lock_entry)
        .collect::<Vec<_>>();

    let graph = build_project_graph_with_options(&manifest_path, ProjectGraphBuildOptions::default())
        .ok();
    let unresolved: Vec<Value> = graph
        .as_ref()
        .map(collect_unresolved_dependencies)
        .unwrap_or_default()
        .into_iter()
        .map(serialize_unresolved_dependency)
        .collect();

    Ok(json!({
        "projectUri": project_uri,
        "declared": declared,
        "locked": locked,
        "unresolved": unresolved,
    }))
}

fn serialize_graph_node(
    node: &beskid_analysis::projects::ProjectGraphNode,
    id: &str,
) -> Value {
    use beskid_analysis::projects::ProjectGraphNode;
    match node {
        ProjectGraphNode::RootProject {
            manifest_path,
            project_root,
            project_name,
            source_root,
            project_kind,
        } => json!({
            "id": id,
            "kind": "root",
            "manifestUri": path_to_uri_string(manifest_path),
            "projectRoot": project_root.display().to_string(),
            "projectName": project_name,
            "sourceRoot": source_root.display().to_string(),
            "projectType": format!("{project_kind:?}"),
        }),
        ProjectGraphNode::ResolvedPathDependency {
            dependency_name,
            manifest_path,
            project_root,
            project_name,
            source_root,
            project_kind,
        } => json!({
            "id": id,
            "kind": "path",
            "dependencyName": dependency_name,
            "manifestUri": path_to_uri_string(manifest_path),
            "projectRoot": project_root.display().to_string(),
            "projectName": project_name,
            "sourceRoot": source_root.display().to_string(),
            "projectType": format!("{project_kind:?}"),
        }),
        ProjectGraphNode::UnresolvedGitDependency {
            dependency_name,
            url,
            rev,
        } => json!({
            "id": id,
            "kind": "git",
            "dependencyName": dependency_name,
            "url": url,
            "rev": rev,
        }),
        ProjectGraphNode::UnresolvedRegistryDependency {
            dependency_name,
            version,
            registry,
        } => json!({
            "id": id,
            "kind": "registry",
            "dependencyName": dependency_name,
            "version": version,
            "registry": registry,
        }),
    }
}

fn serialize_lock_entry(entry: &ProjectLockDependencyEntry) -> Value {
    json!({
        "name": entry.name(),
        "manifest": entry.manifest(),
        "project": entry.project(),
        "sourceRoot": entry.source_root(),
        "materializedRoot": entry.materialized_root(),
        "resolvedVersion": entry.resolved_version().as_deref(),
        "registry": entry.registry().as_deref(),
    })
}

fn serialize_unresolved_dependency(
    item: beskid_analysis::projects::UnresolvedDependency,
) -> Value {
    json!({
        "dependencyName": item.dependency_name,
        "kind": match item.kind {
            UnresolvedDependencyKind::Git => "git",
            UnresolvedDependencyKind::Registry => "registry",
        },
        "descriptor": item.descriptor,
    })
}

fn dependency_source_str(source: DependencySource) -> &'static str {
    match source {
        DependencySource::Path => "path",
        DependencySource::Git => "git",
        DependencySource::Registry => "registry",
    }
}
