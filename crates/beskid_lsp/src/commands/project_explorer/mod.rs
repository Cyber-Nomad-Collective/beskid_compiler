//! LSP `workspace/executeCommand` handlers for workspace and project graph exploration.

mod graph;
mod workspaces;

use std::path::PathBuf;

use serde_json::Value;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::LSPAny;

use crate::commands::pckg_registry::{CMD_GET_CONNECTION_STATUS, CMD_SET_REGISTRY, CMD_VALIDATE_CONNECTION};
use crate::commands::symbol_documentation::CMD_GET_DOCUMENTATION_URI;
use crate::manifest_uri::manifest_path_from_uri_str;
use crate::protocol::execute_args::{missing_args, required_uri_arg};

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
    manifest_path_from_uri_str(uri)
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
            Ok(Some(graph::get_graph(&uri, kind, entry_uri.as_deref(), workspace_uri.as_deref(), compilation_db)?))
        }
        CMD_GET_PROJECT_DEPENDENCIES => {
            let uri = required_uri_arg(&arguments, "projectUri")?;
            Ok(Some(graph::get_project_dependencies(&uri)?))
        }
        _ => Ok(None),
    }
}

pub(crate) fn manifest_path_from_uri(uri: &str) -> Result<PathBuf> {
    manifest_path_from_uri_str(uri).ok_or_else(missing_args)
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
            &root.join("Demo.bws"),
            r#"
workspace {
  name = "Demo"
  resolver = v1
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
            &root.join("apps/demo/demo.bproj"),
            r#"
demo {
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
            &root.join("apps/lib/lib.bproj"),
            r#"
lib {
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
root_manifest=demo.bproj
project_name=demo
dependencies:
- name=lib;manifest=lib.bproj;project=lib;source_root=Src;materialized_root=obj/beskid/deps/src/lib;resolved_version=1.0.0;registry=default
"#,
        );
        (temp, root)
    }

    fn multi_member_workspace_fixture() -> (TempDir, PathBuf) {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path().to_path_buf();
        write(
            &root.join("Superrepo.bws"),
            r#"
workspace {
  name = "Superrepo"
  resolver = v1
}

member "compiler" {
  path = "compiler"
}

member "vscode" {
  path = "beskid_vscode"
}
"#,
        );
        write(
            &root.join("compiler/compiler.bproj"),
            r#"
compiler {
  name = "compiler"
  version = "0.1.0"
}

target "Cli" {
  kind = App
  entry = "Main.bd"
}
"#,
        );
        write(
            &root.join("beskid_vscode/vscode.bproj"),
            r#"
vscode {
  name = "vscode"
  version = "0.1.0"
}

target "Ext" {
  kind = Lib
  entry = "Ext.bd"
}
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
        assert_eq!(members[0]["memberId"], "app");
        assert!(members[0]["uri"].as_str().is_some());
    }

    #[test]
    fn list_workspaces_emits_member_id_for_each_bsol_member() {
        let (_temp, root) = multi_member_workspace_fixture();
        let value = workspaces::list_workspaces(&[root]).expect("list");
        let workspaces = value["workspaces"].as_array().expect("array");
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0]["name"], "Superrepo");
        let members = workspaces[0]["members"].as_array().expect("members");
        assert_eq!(members.len(), 2);

        let compiler = &members[0];
        assert_eq!(compiler["memberId"], "compiler");
        assert_eq!(compiler["name"], "compiler");
        assert_eq!(compiler["path"], "compiler");
        assert!(compiler["uri"].as_str().is_some());

        let vscode = &members[1];
        assert_eq!(vscode["memberId"], "vscode");
        assert_eq!(vscode["name"], "vscode");
        assert_eq!(vscode["path"], "beskid_vscode");
        assert!(vscode["uri"].as_str().is_some());
    }

    #[test]
    fn get_workspace_summary_includes_registries() {
        let (_temp, root) = workspace_fixture();
        let uri = path_to_uri_string(&root.join("Demo.bws"));
        let value = workspaces::get_workspace_summary(&uri).expect("summary");
        assert_eq!(value["name"], "Demo");
        let members = value["members"].as_array().expect("members");
        assert_eq!(members[0]["memberId"], "app");
        let registries = value["registries"].as_array().expect("registries");
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0]["name"], "default");
    }

    #[test]
    fn get_workspace_summary_emits_member_id_for_multi_member_workspace() {
        let (_temp, root) = multi_member_workspace_fixture();
        let uri = path_to_uri_string(&root.join("Superrepo.bws"));
        let value = workspaces::get_workspace_summary(&uri).expect("summary");
        assert_eq!(value["name"], "Superrepo");
        let members = value["members"].as_array().expect("members");
        assert_eq!(members.len(), 2);
        assert_eq!(members[0]["memberId"], "compiler");
        assert_eq!(members[1]["memberId"], "vscode");
        assert!(members[0]["uri"].as_str().is_some());
        assert!(members[1]["uri"].as_str().is_some());
    }

    #[test]
    fn get_graph_returns_mermaid_and_metadata() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/demo.bproj");
        let uri = path_to_uri_string(&project);
        let value = graph::get_graph(&uri, beskid_graph::GraphKind::ProjectDeps, None, None, None).expect("graph");
        let nodes = value["metadata"]["nodes"].as_array().expect("nodes");
        assert!(!nodes.is_empty());
        assert!(value.get("mermaid").and_then(|v| v.as_str()).is_some());
        assert_eq!(value["kind"], "projectDeps");
    }

    #[test]
    fn get_project_dependencies_merges_declared_and_lock() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/demo.bproj");
        let uri = path_to_uri_string(&project);
        let value = graph::get_project_dependencies(&uri).expect("deps");
        let declared = value["declared"].as_array().expect("declared");
        assert_eq!(declared.len(), 1);
        assert_eq!(declared[0]["name"], "lib");
        let locked = value["locked"].as_array().expect("locked");
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0]["resolvedVersion"], "1.0.0");
        let unresolved = value["unresolved"].as_array().expect("unresolved");
        assert!(unresolved.is_empty());
    }

    #[test]
    fn get_project_dependencies_reports_unresolved_when_lock_missing_entry() {
        let (_temp, root) = workspace_fixture();
        let project = root.join("apps/demo/demo.bproj");
        write(
            &project,
            r#"
demo {
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

dependency "missing" {
  source = registry
  version = "1.0.0"
}
"#,
        );
        let uri = path_to_uri_string(&project);
        let value = graph::get_project_dependencies(&uri).expect("deps");
        let unresolved = value["unresolved"]
            .as_array()
            .expect("unresolved")
            .iter()
            .map(|entry| entry.as_str().expect("name").to_string())
            .collect::<Vec<_>>();
        assert_eq!(unresolved, vec!["missing".to_string()]);
    }

    #[test]
    fn project_explorer_command_contract_matches_snapshot() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let snapshot_path =
            manifest_dir.join("../../../beskid_vscode/test/fixtures/lsp-project-explorer-commands.json");
        let text = fs::read_to_string(&snapshot_path)
            .unwrap_or_else(|err| panic!("read contract snapshot at {}: {err}", snapshot_path.display()));
        let snapshot: Value = serde_json::from_str(&text).expect("parse contract snapshot");
        let expected = snapshot["commands"]
            .as_array()
            .expect("commands array")
            .iter()
            .map(|entry| entry.as_str().expect("command name").to_string())
            .collect::<Vec<_>>();
        let actual = PROJECT_EXPLORER_COMMANDS.iter().map(|command| (*command).to_string()).collect::<Vec<_>>();
        assert_eq!(actual, expected, "update beskid_vscode/test/fixtures/lsp-project-explorer-commands.json");
    }

    #[test]
    fn handle_unknown_command_returns_none() {
        let (_temp, root) = workspace_fixture();
        let result = handle_project_explorer_command("beskid.unknown", None, &[root], None).expect("ok");
        assert!(result.is_none());
    }

    #[test]
    fn handle_list_workspaces_via_command_router() {
        let (_temp, root) = workspace_fixture();
        let result =
            handle_project_explorer_command(CMD_LIST_WORKSPACES, None, &[root], None).expect("ok").expect("payload");
        let value = serde_json::to_value(&result).expect("json");
        assert!(value.get("workspaces").is_some());
    }

    #[test]
    fn handle_get_workspace_summary_requires_uri() {
        let (_temp, root) = workspace_fixture();
        let err =
            handle_project_explorer_command(CMD_GET_WORKSPACE_SUMMARY, None, &[root], None).expect_err("missing args");
        assert!(format!("{err}").contains("missing"));
    }

    #[test]
    fn project_explorer_commands_include_refresh_workspace() {
        assert!(PROJECT_EXPLORER_COMMANDS.contains(&"beskid.refreshWorkspace"));
    }

    #[test]
    fn focused_project_from_configuration_paths() {
        let path = std::env::temp_dir().join("focus-test/demo.bproj");
        let uri = format!("file://{}", path.display());
        let settings = serde_json::json!({ "beskid": { "focusedProjectUri": uri } });
        let focused = focused_project_from_configuration(&settings).expect("some");
        assert!(focused.is_some());
        let settings_legacy = serde_json::json!({ "beskid": { "selectedProjectUri": uri.clone() } });
        assert!(focused_project_from_configuration(&settings_legacy).expect("some").is_some());
        let settings_nested = serde_json::json!({
            "beskid": { "project": { "focusedProjectUri": uri } }
        });
        assert!(focused_project_from_configuration(&settings_nested).expect("some").is_some());
    }
}
