//! LSP `workspace/executeCommand` handlers for workspace and project graph exploration.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    ProjectGraphBuildOptions, ProjectLockDependencyEntry, UnresolvedDependencyKind,
    WORKSPACE_FILE_NAME, build_project_graph_with_options, collect_unresolved_dependencies,
    load_project_lock_dependencies, parse_manifest, parse_workspace_manifest, DependencySource,
};
use daggy::petgraph::visit::EdgeRef;
use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::LSPAny;
use walkdir::WalkDir;

use crate::workspace_scan::{path_to_uri, uri_to_path};

const CMD_LIST_WORKSPACES: &str = "beskid.listWorkspaces";
const CMD_GET_WORKSPACE_SUMMARY: &str = "beskid.getWorkspaceSummary";
const CMD_GET_PROJECT_GRAPH: &str = "beskid.getProjectGraph";
const CMD_GET_PROJECT_DEPENDENCIES: &str = "beskid.getProjectDependencies";

pub const PROJECT_EXPLORER_COMMANDS: &[&str] = &[
    "beskid.refreshWorkspace",
    CMD_LIST_WORKSPACES,
    CMD_GET_WORKSPACE_SUMMARY,
    CMD_GET_PROJECT_GRAPH,
    CMD_GET_PROJECT_DEPENDENCIES,
];

/// Parse `focusedProjectUri` from LSP initialization options or workspace settings JSON.
pub fn focused_project_from_value(value: &Value) -> Option<PathBuf> {
    let uri = value
        .get("focusedProjectUri")
        .or_else(|| value.get("selectedProjectUri"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    uri_to_path_from_string(uri)
}

/// Extract optional `focusedProjectUri` from `didChangeConfiguration` settings.
pub fn focused_project_from_configuration(settings: &Value) -> Option<Option<PathBuf>> {
    let beskid = settings.get("beskid")?;
    if let Some(uri) = focused_project_from_value(beskid) {
        return Some(Some(uri));
    }
    let project = beskid.get("project")?;
    if project.is_null() {
        return Some(None);
    }
    Some(focused_project_from_value(project))
}

pub fn handle_project_explorer_command(
    command: &str,
    arguments: Option<Vec<Value>>,
    workspace_roots: &[PathBuf],
) -> Result<Option<LSPAny>> {
    match command {
        CMD_LIST_WORKSPACES => Ok(Some(list_workspaces(workspace_roots)?.into())),
        CMD_GET_WORKSPACE_SUMMARY => {
            let uri = required_uri_arg(&arguments, "workspaceUri")?;
            Ok(Some(get_workspace_summary(&uri)?.into()))
        }
        CMD_GET_PROJECT_GRAPH => {
            let uri = required_uri_arg(&arguments, "projectUri")?;
            Ok(Some(get_project_graph(&uri)?.into()))
        }
        CMD_GET_PROJECT_DEPENDENCIES => {
            let uri = required_uri_arg(&arguments, "projectUri")?;
            Ok(Some(get_project_dependencies(&uri)?.into()))
        }
        _ => Ok(None),
    }
}

fn required_uri_arg(arguments: &Option<Vec<Value>>, key: &str) -> Result<String> {
    let args = arguments.as_ref().ok_or_else(missing_args)?;
    if args.is_empty() {
        return Err(missing_args());
    }
    if let Some(uri) = args[0].as_str() {
        return Ok(uri.to_string());
    }
    if let Some(obj) = args[0].as_object() {
        if let Some(uri) = obj.get(key).and_then(Value::as_str) {
            return Ok(uri.to_string());
        }
    }
    Err(missing_args())
}

fn missing_args() -> tower_lsp_server::jsonrpc::Error {
    tower_lsp_server::jsonrpc::Error::invalid_params("missing command arguments")
}

fn uri_to_path_from_string(uri: &str) -> Option<PathBuf> {
    use std::str::FromStr;
    use tower_lsp_server::ls_types::Uri;
    uri_to_path(&Uri::from_str(uri).ok()?)
}

fn manifest_path_from_uri(uri: &str) -> Result<PathBuf> {
    uri_to_path_from_string(uri).ok_or_else(missing_args)
}

fn path_uri_string(path: &Path) -> String {
    path_to_uri(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("file://{}", path.display()))
}

fn list_workspaces(workspace_roots: &[PathBuf]) -> Result<Value> {
    let mut workspaces = Vec::new();
    let mut seen = HashSet::new();

    for root in workspace_roots {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    !e.file_name()
                        .to_str()
                        .map(should_skip_dir_for_scan)
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .filter_map(|entry| entry.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) != Some(WORKSPACE_FILE_NAME) {
                continue;
            }
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => path.to_path_buf(),
            };
            if !seen.insert(canonical.clone()) {
                continue;
            }
            if let Some(ws) = workspace_entry(&canonical) {
                workspaces.push(ws);
            }
        }
    }

    workspaces.sort_by(|a, b| a["uri"].as_str().cmp(&b["uri"].as_str()));
    Ok(json!({ "workspaces": workspaces }))
}

fn workspace_entry(workspace_manifest_path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(workspace_manifest_path).ok()?;
    let manifest = parse_workspace_manifest(&text).ok()?;
    let workspace_dir = workspace_manifest_path.parent()?;
    let workspace_uri = path_uri_string(workspace_manifest_path);

    let mut members = Vec::new();
    for member in manifest.members {
        let member_manifest = workspace_dir.join(&member.path).join("Project.proj");
        let member_uri = if member_manifest.is_file() {
            Some(path_uri_string(&member_manifest))
        } else {
            None
        };
        members.push(json!({
            "name": member.name,
            "path": member.path,
            "uri": member_uri,
        }));
    }

    Some(json!({
        "uri": workspace_uri,
        "name": manifest.workspace.name,
        "members": members,
    }))
}

fn get_workspace_summary(workspace_uri: &str) -> Result<Value> {
    let path = manifest_path_from_uri(workspace_uri)?;
    let text = std::fs::read_to_string(&path).map_err(|_| missing_args())?;
    let manifest = parse_workspace_manifest(&text).map_err(|_| missing_args())?;
    let workspace_dir = path.parent().ok_or_else(missing_args)?;

    let mut members = Vec::new();
    for member in &manifest.members {
        let member_manifest = workspace_dir.join(&member.path).join("Project.proj");
        members.push(json!({
            "name": member.name,
            "path": member.path,
            "uri": member_manifest.is_file().then(|| path_uri_string(&member_manifest)),
        }));
    }

    let mut registries = Vec::new();
    for registry in &manifest.registries {
        registries.push(json!({
            "name": registry.name,
            "url": registry.url,
        }));
    }

    Ok(json!({
        "workspaceUri": workspace_uri,
        "name": manifest.workspace.name,
        "resolver": manifest.workspace.resolver,
        "members": members,
        "registries": registries,
    }))
}

fn get_project_graph(project_uri: &str) -> Result<Value> {
    let manifest_path = manifest_path_from_uri(project_uri)?;
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
        .map(|item| {
            json!({
                "dependencyName": item.dependency_name,
                "kind": match item.kind {
                    UnresolvedDependencyKind::Git => "git",
                    UnresolvedDependencyKind::Registry => "registry",
                },
                "descriptor": item.descriptor,
            })
        })
        .collect();

    Ok(json!({
        "projectUri": project_uri,
        "nodes": nodes,
        "edges": edges,
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
            "manifestUri": path_uri_string(manifest_path),
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
            "manifestUri": path_uri_string(manifest_path),
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

fn get_project_dependencies(project_uri: &str) -> Result<Value> {
    let manifest_path = manifest_path_from_uri(project_uri)?;
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
        .map(|item| {
            json!({
                "dependencyName": item.dependency_name,
                "kind": match item.kind {
                    UnresolvedDependencyKind::Git => "git",
                    UnresolvedDependencyKind::Registry => "registry",
                },
                "descriptor": item.descriptor,
            })
        })
        .collect();

    Ok(json!({
        "projectUri": project_uri,
        "declared": declared,
        "locked": locked,
        "unresolved": unresolved,
    }))
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

fn dependency_source_str(source: DependencySource) -> &'static str {
    match source {
        DependencySource::Path => "path",
        DependencySource::Git => "git",
        DependencySource::Registry => "registry",
    }
}

use crate::workspace_scan::should_skip_dir_for_scan;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, content).expect("write");
    }

    fn workspace_fixture() -> (TempDir, PathBuf) {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path().to_path_buf();
        write(
            &root.join("Workspace.proj"),
            r#"
workspace {
  name = "Demo"
  resolver = "v1"
}

member "app" {
  path = "apps/demo"
}

registry "default" {
  url = "https://pckg.example.test"
}
"#,
        );
        write(
            &root.join("apps/demo/Project.proj"),
            r#"
project {
  name = "demo"
  version = "0.1.0"
}

target "App" {
  kind = App
  entry = "Main.bd"
}

dependency "lib" {
  source = path
  path = "../lib"
}
"#,
        );
        write(
            &root.join("apps/lib/Project.proj"),
            r#"
project {
  name = "lib"
  version = "0.1.0"
}

target "Lib" {
  kind = Lib
  entry = "Lib.bd"
}
"#,
        );
        write(
            &root.join("apps/demo/Project.lock"),
            r#"# Project.lock v1
root_manifest=Project.proj
project_name=demo
dependencies:
- name=lib;manifest=Project.proj;project=lib;source_root=Src;materialized_root=obj/beskid/deps/src/lib;resolved_version=1.0.0;registry=default
"#,
        );
        (temp, root)
    }

    #[test]
    fn list_workspaces_json_shape() {
        let (_temp, root) = workspace_fixture();
        let value = list_workspaces(&[root]).expect("list");
        let workspaces = value["workspaces"].as_array().expect("array");
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0]["name"], "Demo");
        let members = workspaces[0]["members"].as_array().expect("members");
        assert_eq!(members.len(), 1);
        assert_eq!(members[0]["name"], "app");
        assert!(members[0]["uri"].as_str().is_some());
    }

    #[test]
    fn get_workspace_summary_includes_registries() {
        let (_temp, root) = workspace_fixture();
        let uri = path_uri_string(&root.join("Workspace.proj"));
        let value = get_workspace_summary(&uri).expect("summary");
        assert_eq!(value["name"], "Demo");
        let registries = value["registries"].as_array().expect("registries");
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0]["name"], "default");
    }

    #[test]
    fn get_project_graph_has_nodes_and_edges() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/Project.proj");
        let uri = path_uri_string(&project);
        let value = get_project_graph(&uri).expect("graph");
        let nodes = value["nodes"].as_array().expect("nodes");
        assert!(nodes.len() >= 2);
        let edges = value["edges"].as_array().expect("edges");
        assert!(!edges.is_empty());
        assert!(value["unresolved"].as_array().is_some());
    }

    #[test]
    fn get_project_dependencies_merges_declared_and_lock() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/Project.proj");
        let uri = path_uri_string(&project);
        let value = get_project_dependencies(&uri).expect("deps");
        let declared = value["declared"].as_array().expect("declared");
        assert_eq!(declared.len(), 1);
        assert_eq!(declared[0]["name"], "lib");
        let locked = value["locked"].as_array().expect("locked");
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0]["resolvedVersion"], "1.0.0");
    }

    #[test]
    fn focused_project_from_configuration_paths() {
        let path = std::env::temp_dir().join("focus-test/Project.proj");
        let uri = format!("file://{}", path.display());
        let settings = json!({ "beskid": { "focusedProjectUri": uri } });
        let focused = focused_project_from_configuration(&settings).expect("some");
        assert!(focused.is_some());
        let settings_legacy = json!({ "beskid": { "selectedProjectUri": uri.clone() } });
        assert!(focused_project_from_configuration(&settings_legacy)
            .expect("some")
            .is_some());
    }
}
