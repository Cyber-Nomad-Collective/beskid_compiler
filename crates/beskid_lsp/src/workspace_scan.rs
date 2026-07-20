//! Walk workspace roots to index `.bd` / `.bproj` / `.bws` files and publish disk-backed diagnostics.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use beskid_analysis::projects::is_workspace_manifest_path;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, Semaphore};
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::Uri;
use url::Url;
use walkdir::WalkDir;

use crate::diagnostics::analyze_document;
use crate::protocol::status::{idle_status, send_beskid_status, workspace_scan_status};
use crate::session::diagnostics_bridge::analyze_document_for_state;
use crate::session::lifecycle::{
    build_document, rebuild_open_document_analysis, set_disk_snapshot,
};
use crate::session::project_context::{cached_compilation_context, invalidate_compilation_cache};
use crate::session::startup::signal_initial_scan_complete;
use crate::session::store::{Document, State};

const MAX_CONCURRENT_READS: usize = 24;
const STATUS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

fn uri_from_path(path: &Path) -> Option<Uri> {
    let url = Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

pub(crate) fn should_skip_dir_for_scan(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".beskid" | "out" | "bin" | "obj" | ".vs"
    )
}

fn is_scannable_extension(ext: &str) -> bool {
    matches!(ext, "bd" | "bproj" | "bws")
}

fn is_manifest_extension(ext: &str) -> bool {
    matches!(ext, "bproj" | "bws")
}

async fn maybe_emit_scan_progress(
    client: &Client,
    last_emit: &mut Option<Instant>,
    processed: u32,
    total: u32,
    detail: Option<String>,
) {
    let now = Instant::now();
    let elapsed_ok = last_emit
        .map(|t| now.duration_since(t) >= STATUS_EMIT_INTERVAL)
        .unwrap_or(true);
    let milestone = processed == 0 || processed == total || processed.is_multiple_of(25);
    if !milestone && !elapsed_ok {
        return;
    }
    *last_emit = Some(now);
    send_beskid_status(client, workspace_scan_status(processed, total, detail)).await;
}

async fn emit_scan_idle(client: &Client) {
    send_beskid_status(client, idle_status()).await;
}

/// Recursively index `root` for Beskid sources, publish diagnostics for closed files, then emit idle status.
pub async fn scan_workspace(
    client: &Client,
    state: &RwLock<State>,
    root: &Path,
    focused_project: Option<&Path>,
) {
    let mut paths: Vec<PathBuf> = Vec::new();
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
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(is_scannable_extension)
        {
            paths.push(entry.path().to_path_buf());
        }
    }

    let focus_root = focused_project.and_then(|manifest| manifest.parent());
    paths.sort_by(|a, b| {
        let a_focus = focus_root.is_some_and(|focus| a.starts_with(focus));
        let b_focus = focus_root.is_some_and(|focus| b.starts_with(focus));
        b_focus
            .cmp(&a_focus)
            .then_with(|| {
                let a_manifest = a
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(is_manifest_extension);
                let b_manifest = b
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(is_manifest_extension);
                b_manifest.cmp(&a_manifest)
            })
            .then_with(|| a.as_path().cmp(b.as_path()))
    });

    invalidate_compilation_cache(state).await;

    let total = paths.len() as u32;
    let mut last_emit = None;
    if total > 0 {
        maybe_emit_scan_progress(
            client,
            &mut last_emit,
            0,
            total,
            Some(root.display().to_string()),
        )
        .await;
    }

    let sem = Semaphore::new(MAX_CONCURRENT_READS);
    let mut processed: u32 = 0;
    for path in paths {
        let _permit = match sem.acquire().await {
            Ok(p) => p,
            Err(_) => continue,
        };
        processed = processed.saturating_add(1);
        let detail = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| path.to_str().map(ToString::to_string));
        maybe_emit_scan_progress(client, &mut last_emit, processed, total, detail).await;

        let Some(uri) = uri_from_path(&path) else {
            continue;
        };
        let skip = {
            let s = state.read().await;
            s.docs.contains_key(&uri)
        };
        if skip {
            continue;
        }
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let doc = Document {
            version: 0,
            text: text.clone(),
            analysis_cache_version: 0,
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
        };
        let diagnostics = analyze_document(None, &uri, &text, None);
        set_disk_snapshot(state, uri.clone(), doc).await;
        client.publish_diagnostics(uri, diagnostics, Some(0)).await;
    }

    signal_initial_scan_complete(state).await;

    rebuild_open_document_analysis(state).await;

    let mut stale: Vec<Uri> = Vec::new();
    {
        let s = state.read().await;
        let root_prefix = root.to_string_lossy();
        for uri in s.workspace_index.keys() {
            if let Some(p) = uri_to_path(uri) {
                let lossy = p.to_string_lossy();
                if !lossy.starts_with(root_prefix.as_ref()) {
                    continue;
                }
                if !p.exists() {
                    stale.push(uri.clone());
                }
            }
        }
    }
    for uri in stale {
        clear_disk_snapshot(client, state, &uri).await;
    }

    emit_scan_idle(client).await;
}

/// Remove a workspace-indexed document and clear its diagnostics.
pub async fn clear_disk_snapshot(client: &Client, state: &RwLock<State>, uri: &Uri) {
    state.write().await.workspace_index.remove(uri);
    client
        .publish_diagnostics(uri.clone(), Vec::new(), None)
        .await;
}

/// Best-effort `file://` URI to local path (for workspace scanning and file watchers).
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let url = Url::parse(uri.as_str()).ok()?;
    url.to_file_path().ok()
}

/// Map a local filesystem path to an LSP `file://` URI.
pub fn path_to_uri(path: &Path) -> Option<Uri> {
    uri_from_path(path)
}

/// Map a local path to a `file://` URI string (fallback uses `path.display()`).
pub fn path_to_uri_string(path: &Path) -> String {
    path_to_uri(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("file://{}", path.display()))
}

/// Parse a URI string to a local filesystem path.
pub fn path_from_uri_string(uri: &str) -> Option<PathBuf> {
    Uri::from_str(uri).ok().and_then(|u| uri_to_path(&u))
}

/// Discover `.bws` workspace manifests under workspace roots (sorted, deduplicated).
pub fn discover_workspace_manifest_paths(workspace_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
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
            if !is_workspace_manifest_path(path) {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if seen.insert(canonical.clone()) {
                manifests.push(canonical);
            }
        }
    }
    manifests.sort();
    manifests
}

/// Clears closed-file workspace cache and diagnostics for every indexed URI under `root`.
pub async fn clear_closed_workspace_under_root(
    client: &Client,
    state: &RwLock<State>,
    root: &Path,
) {
    let root_key = root.to_string_lossy().to_string();
    let mut remove: Vec<Uri> = Vec::new();
    {
        let s = state.read().await;
        for uri in s.workspace_index.keys() {
            if let Some(p) = uri_to_path(uri)
                && p.to_string_lossy().starts_with(root_key.as_str())
                && !s.docs.contains_key(uri)
            {
                remove.push(uri.clone());
            }
        }
    }
    for uri in remove {
        clear_disk_snapshot(client, state, &uri).await;
    }
}

/// Re-read changed paths on disk when buffers are closed; may invalidate compilation cache on manifest edits.
pub async fn refresh_after_disk_change(
    client: &Client,
    state: &RwLock<State>,
    changed_paths: &[PathBuf],
) {
    if changed_paths.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_manifest_extension)
    }) {
        invalidate_compilation_cache(state).await;
        rebuild_open_document_analysis(state).await;
    }
    for path in changed_paths {
        let Some(uri) = uri_from_path(path) else {
            continue;
        };
        let open = {
            let s = state.read().await;
            s.docs.contains_key(&uri)
        };
        if open {
            continue;
        }
        let Ok(text) = tokio::fs::read_to_string(path).await else {
            clear_disk_snapshot(client, state, &uri).await;
            continue;
        };
        let doc = build_document(state, &uri, 0, text).await;
        let compilation_context = if path.extension().and_then(|e| e.to_str()) == Some("bd") {
            cached_compilation_context(state, path).await
        } else {
            None
        };
        let diagnostics =
            analyze_document_for_state(state, &uri, &doc.text, compilation_context.as_ref()).await;
        set_disk_snapshot(state, uri.clone(), doc).await;
        client.publish_diagnostics(uri, diagnostics, Some(0)).await;
    }
}

/// After `didClose`, reload disk contents into the workspace index when the file still exists.
pub async fn hydrate_disk_after_close(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let Some(path) = uri_to_path(uri) else {
        client
            .publish_diagnostics(uri.clone(), Vec::new(), None)
            .await;
        return;
    };
    if !path.exists() {
        clear_disk_snapshot(client, state, uri).await;
        return;
    }
    let Ok(text) = tokio::fs::read_to_string(&path).await else {
        clear_disk_snapshot(client, state, uri).await;
        return;
    };
    let doc = build_document(state, uri, 0, text).await;
    let compilation_context = if path.extension().and_then(|e| e.to_str()) == Some("bd") {
        cached_compilation_context(state, &path).await
    } else {
        None
    };
    let diagnostics =
        analyze_document_for_state(state, uri, &doc.text, compilation_context.as_ref()).await;
    set_disk_snapshot(state, uri.clone(), doc).await;
    client
        .publish_diagnostics(uri.clone(), diagnostics, Some(0))
        .await;
}
