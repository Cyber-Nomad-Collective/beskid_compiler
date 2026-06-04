//! LSP `workspace/executeCommand` handlers for workspace and project graph exploration.

mod graph;
mod workspaces;

use std::path::PathBuf;

use serde_json::Value;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::LSPAny;

use crate::commands::pckg_registry::{
    CMD_GET_CONNECTION_STATUS, CMD_SET_REGISTRY, CMD_VALIDATE_CONNECTION,
};
use crate::commands::symbol_documentation::CMD_GET_DOCUMENTATION_URI;
use crate::protocol::execute_args::{missing_args, required_uri_arg};
use crate::workspace_scan::path_from_uri_string;

const CMD_LIST_WORKSPACES: &str = "beskid.listWorkspaces";
const CMD_GET_WORKSPACE_SUMMARY: &str = "beskid.getWorkspaceSummary";
const CMD_GET_GRAPH: &str = "beskid.getGraph";
const CMD_GET_PROJECT_DEPENDENCIES: &str = "beskid.getProjectDependencies";

pub const PROJECT_EXPLORER_COMMANDS: &[&str] = &[
    "beskid.refreshWorkspace",
    CMD_LIST_WORKSPACES,
    CMD_GET_WORKSPACE_SUMMARY,
    CMD_GET_GRAPH,
    CMD_GET_PROJECT_DEPENDENCIES,
    CMD_GET_CONNECTION_STATUS,
    CMD_SET_REGISTRY,
    CMD_VALIDATE_CONNECTION,
    CMD_GET_DOCUMENTATION_URI,
];

/// Parse `focusedProjectUri` from LSP initialization options or workspace settings JSON.
pub fn focused_project_from_value(value: &Value) -> Option<PathBuf> {
    let uri = value
        .get("focusedProjectUri")
        .or_else(|| value.get("selectedProjectUri"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    path_from_uri_string(uri)
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
    compilation_db: Option<&mut beskid_queries::BeskidDatabase>,
) -> Result<Option<LSPAny>> {
    match command {
        CMD_LIST_WORKSPACES => Ok(Some(workspaces::list_workspaces(workspace_roots)?)),
        CMD_GET_WORKSPACE_SUMMARY => {
            let uri = required_uri_arg(&arguments, "workspaceUri")?;
            Ok(Some(workspaces::get_workspace_summary(&uri)?))
        }
        CMD_GET_GRAPH => {
            let uri = required_uri_arg(&arguments, "projectUri")?;
            let kind = graph::graph_kind_from_args(arguments.as_deref());
            let entry_uri = graph::optional_uri_arg(arguments.as_deref(), "entryUri");
            let workspace_uri = graph::optional_uri_arg(arguments.as_deref(), "workspaceUri");
            Ok(Some(
                graph::get_graph(&uri, kind, entry_uri.as_deref(), workspace_uri.as_deref(), compilation_db)?,
            ))
        }
        CMD_GET_PROJECT_DEPENDENCIES => {
            let uri = required_uri_arg(&arguments, "projectUri")?;
            Ok(Some(graph::get_project_dependencies(&uri)?))
        }
        _ => Ok(None),
    }
}

pub(crate) fn manifest_path_from_uri(uri: &str) -> Result<PathBuf> {
    path_from_uri_string(uri).ok_or_else(missing_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    use crate::workspace_scan::path_to_uri_string;

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
        let value = workspaces::list_workspaces(&[root]).expect("list");
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
        let uri = path_to_uri_string(&root.join("Workspace.proj"));
        let value = workspaces::get_workspace_summary(&uri).expect("summary");
        assert_eq!(value["name"], "Demo");
        let registries = value["registries"].as_array().expect("registries");
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0]["name"], "default");
    }

    #[test]
    fn get_graph_returns_mermaid_and_metadata() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/Project.proj");
        let uri = path_to_uri_string(&project);
        let value = graph::get_graph(&uri, beskid_graph::GraphKind::ProjectDeps, None, None, None)
            .expect("graph");
        let nodes = value["metadata"]["nodes"].as_array().expect("nodes");
        assert!(!nodes.is_empty());
        assert!(value.get("mermaid").and_then(|v| v.as_str()).is_some());
        assert_eq!(value["kind"], "projectDeps");
    }

    #[test]
    fn get_project_dependencies_merges_declared_and_lock() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/Project.proj");
        let uri = path_to_uri_string(&project);
        let value = graph::get_project_dependencies(&uri).expect("deps");
        let declared = value["declared"].as_array().expect("declared");
        assert_eq!(declared.len(), 1);
        assert_eq!(declared[0]["name"], "lib");
        let locked = value["locked"].as_array().expect("locked");
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0]["resolvedVersion"], "1.0.0");
    }

    #[test]
    fn handle_unknown_command_returns_none() {
        let (_temp, root) = workspace_fixture();
        let result =
            handle_project_explorer_command("beskid.unknown", None, &[root], None).expect("ok");
        assert!(result.is_none());
    }

    #[test]
    fn handle_list_workspaces_via_command_router() {
        let (_temp, root) = workspace_fixture();
        let result = handle_project_explorer_command(CMD_LIST_WORKSPACES, None, &[root], None)
            .expect("ok")
            .expect("payload");
        let value = serde_json::to_value(&result).expect("json");
        assert!(value.get("workspaces").is_some());
    }

    #[test]
    fn handle_get_workspace_summary_requires_uri() {
        let (_temp, root) = workspace_fixture();
        let err = handle_project_explorer_command(CMD_GET_WORKSPACE_SUMMARY, None, &[root], None)
            .expect_err("missing args");
        assert!(format!("{err}").contains("missing"));
    }

    #[test]
    fn project_explorer_commands_include_refresh_workspace() {
        assert!(PROJECT_EXPLORER_COMMANDS.contains(&"beskid.refreshWorkspace"));
    }

    #[test]
    fn focused_project_from_configuration_paths() {
        let path = std::env::temp_dir().join("focus-test/Project.proj");
        let uri = format!("file://{}", path.display());
        let settings = serde_json::json!({ "beskid": { "focusedProjectUri": uri } });
        let focused = focused_project_from_configuration(&settings).expect("some");
        assert!(focused.is_some());
        let settings_legacy = serde_json::json!({ "beskid": { "selectedProjectUri": uri.clone() } });
        assert!(focused_project_from_configuration(&settings_legacy)
            .expect("some")
            .is_some());
    }
}
