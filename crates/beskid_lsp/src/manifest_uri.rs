//! Manifest URI detection for `.bproj` / `.bws` buffers (delegates to analysis discovery).

use std::path::PathBuf;

use beskid_analysis::projects::{is_project_manifest_path, is_workspace_manifest_path};
use tower_lsp_server::ls_types::Uri;

use crate::workspace_scan::{path_from_uri_string, uri_to_path};

pub fn is_project_manifest_uri(uri: &Uri) -> bool {
    uri_to_path(uri).is_some_and(|path| is_project_manifest_path(&path))
}

pub fn is_workspace_manifest_uri(uri: &Uri) -> bool {
    uri_to_path(uri).is_some_and(|path| is_workspace_manifest_path(&path))
}

pub fn is_manifest_uri(uri: &Uri) -> bool {
    is_project_manifest_uri(uri) || is_workspace_manifest_uri(uri)
}

pub fn manifest_path_from_uri_str(uri: &str) -> Option<PathBuf> {
    path_from_uri_string(uri)
}
