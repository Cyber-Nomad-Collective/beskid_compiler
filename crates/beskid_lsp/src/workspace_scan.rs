use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, Semaphore};
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::Uri;
use url::Url;
use walkdir::WalkDir;

use crate::diagnostics::analyze_document;
use crate::protocol::status::{send_beskid_status, BeskidStatusParams};
use crate::session::lifecycle::{build_document, set_disk_snapshot};
use crate::session::store::State;

const MAX_CONCURRENT_READS: usize = 24;
const STATUS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

fn uri_from_path(path: &Path) -> Option<Uri> {
    let url = Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".beskid" | "out" | "bin" | "obj" | ".vs"
    )
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
    let milestone =
        processed == 0 || processed == total || processed.is_multiple_of(25);
    if !milestone && !elapsed_ok {
        return;
    }
    *last_emit = Some(now);
    send_beskid_status(
        client,
        BeskidStatusParams {
            source: "lsp".into(),
            phase: "workspace_scan".into(),
            message: detail,
            current: Some(processed),
            total: Some(total),
            active: true,
        },
    )
    .await;
}

async fn emit_scan_idle(client: &Client) {
    send_beskid_status(
        client,
        BeskidStatusParams {
            source: "lsp".into(),
            phase: "idle".into(),
            message: None,
            current: None,
            total: None,
            active: false,
        },
    )
    .await;
}

pub async fn scan_workspace(client: &Client, state: &RwLock<State>, root: &Path) {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                !e.file_name()
                    .to_str()
                    .map(should_skip_dir)
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
            .is_some_and(|ext| ext == "bd" || ext == "proj")
        {
            paths.push(entry.path().to_path_buf());
        }
    }

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
        let doc = build_document(&uri, 0, text);
        let diagnostics = analyze_document(&uri, &doc.text, doc.analysis.as_ref());
        set_disk_snapshot(state, uri.clone(), doc).await;
        client
            .publish_diagnostics(uri, diagnostics, Some(0))
            .await;
    }

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

pub async fn clear_disk_snapshot(client: &Client, state: &RwLock<State>, uri: &Uri) {
    state.write().await.workspace_index.remove(uri);
    client
        .publish_diagnostics(uri.clone(), Vec::new(), None)
        .await;
}

pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let url = Url::parse(uri.as_str()).ok()?;
    url.to_file_path().ok()
}

/// Clears closed-file workspace cache and diagnostics for every indexed URI under `root`.
pub async fn clear_closed_workspace_under_root(client: &Client, state: &RwLock<State>, root: &Path) {
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

pub async fn refresh_after_disk_change(
    client: &Client,
    state: &RwLock<State>,
    changed_paths: &[PathBuf],
) {
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
        let doc = build_document(&uri, 0, text);
        let diagnostics = analyze_document(&uri, &doc.text, doc.analysis.as_ref());
        set_disk_snapshot(state, uri.clone(), doc).await;
        client
            .publish_diagnostics(uri, diagnostics, Some(0))
            .await;
    }
}

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
    let doc = build_document(uri, 0, text);
    let diagnostics = analyze_document(uri, &doc.text, doc.analysis.as_ref());
    set_disk_snapshot(state, uri.clone(), doc).await;
    client
        .publish_diagnostics(uri.clone(), diagnostics, Some(0))
        .await;
}
