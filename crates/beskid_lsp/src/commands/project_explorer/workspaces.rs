//! Workspace listing and summary execute-command payloads.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    is_workspace_manifest_path, parse_workspace_manifest, project_manifest_for_member_dir,
};
use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::Result;
use walkdir::WalkDir;

use crate::protocol::execute_args::missing_args;
use crate::workspace_scan::{path_to_uri_string, should_skip_dir_for_scan};

pub(crate) fn list_workspaces(workspace_roots: &[PathBuf]) -> Result<Value> {
    let mut workspaces = Vec::new();
    let mut seen = HashSet::new();

    for root in workspace_roots {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    !e.file_name().to_str().map(should_skip_dir_for_scan).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter_map(|entry| entry.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if !is_workspace_manifest_path(path) {
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

pub(crate) fn get_workspace_summary(workspace_uri: &str) -> Result<Value> {
    let path = super::manifest_path_from_uri(workspace_uri)?;
    let text = std::fs::read_to_string(&path).map_err(|_| missing_args())?;
    let manifest = parse_workspace_manifest(&text).map_err(|_| missing_args())?;
    let workspace_dir = path.parent().ok_or_else(missing_args)?;

    let mut members = Vec::new();
    for member in &manifest.members {
        let member_dir = workspace_dir.join(&member.path);
        let member_uri = project_manifest_for_member_dir(&member_dir)
            .ok()
            .map(|member_manifest| path_to_uri_string(&member_manifest));
        members.push(json!({
            "memberId": member.name,
            "name": member.name,
            "path": member.path,
            "uri": member_uri,
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

fn workspace_entry(workspace_manifest_path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(workspace_manifest_path).ok()?;
    let manifest = parse_workspace_manifest(&text).ok()?;
    let workspace_dir = workspace_manifest_path.parent()?;
    let workspace_uri = path_to_uri_string(workspace_manifest_path);

    let mut members = Vec::new();
    for member in manifest.members {
        let member_dir = workspace_dir.join(&member.path);
        let member_uri = project_manifest_for_member_dir(&member_dir)
            .ok()
            .map(|member_manifest| path_to_uri_string(&member_manifest));
        members.push(json!({
            "memberId": member.name,
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
