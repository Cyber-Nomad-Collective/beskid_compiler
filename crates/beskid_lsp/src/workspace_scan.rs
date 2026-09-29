//! Walk workspace roots to index `.bd` / `.bproj` / `.bws` / `.bsol` files and publish disk-backed diagnostics.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use beskid_analysis::projects::{PROJECT_LOCK_FILE_NAME, is_workspace_manifest_path};
use tokio::sync::{RwLock, Semaphore};
use tower_lsp_server::{Client, ls_types::Uri};
use url::Url;
use walkdir::WalkDir;

use crate::{
    diagnostics::{collect_syntax_diagnostics, lsp_diagnostics_from_syntax},
    protocol::status::{idle_status, send_beskid_status, workspace_scan_status},
    session::{
        lifecycle::{
            build_document, build_initial_workspace_document, publish_diagnostics_for_uri,
            rebuild_open_document_syntax_facts, set_disk_snapshot,
        },
        project_context::invalidate_compilation_cache,
        startup::signal_initial_scan_complete,
        store::{Document, State},
    },
};

const MAX_CONCURRENT_READS: usize = 24;
const STATUS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

fn uri_from_path(path: &Path) -> Option<Uri> {
    let url = Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

pub(crate) fn should_skip_dir_for_scan(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules" | ".beskid" | "out" | "bin" | "obj" | ".vs")
}

fn is_scannable_extension(ext: &str) -> bool {
    matches!(ext, "bd" | "bproj" | "bws" | "bsol")
}

fn is_manifest_extension(ext: &str) -> bool {
    matches!(ext, "bproj" | "bws")
}

fn is_lockfile_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(PROJECT_LOCK_FILE_NAME)
}

async fn maybe_emit_scan_progress(
    client: &Client,
    last_emit: &mut Option<Instant>,
    processed: u32,
    total: u32,
    detail: Option<String>,
) {
    let now = Instant::now();
    let elapsed_ok = last_emit.map(|t| now.duration_since(t) >= STATUS_EMIT_INTERVAL).unwrap_or(true);
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
pub async fn scan_workspace(client: &Client, state: &RwLock<State>, root: &Path, focused_project: Option<&Path>) {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                !e.file_name().to_str().map(should_skip_dir_for_scan).unwrap_or(false)
            } else {
                true
            }
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if entry.path().extension().and_then(|ext| ext.to_str()).is_some_and(is_scannable_extension) {
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
                let a_manifest = a.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension);
                let b_manifest = b.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension);
                b_manifest.cmp(&a_manifest)
            })
            .then_with(|| a.as_path().cmp(b.as_path()))
    });

    invalidate_compilation_cache(state).await;

    let total = paths.len() as u32;
    let mut last_emit = None;
    if total > 0 {
        maybe_emit_scan_progress(client, &mut last_emit, 0, total, Some(root.display().to_string())).await;
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
        let doc =
            if is_manifest_extension(path.extension().and_then(|extension| extension.to_str()).unwrap_or_default())
                || path.extension().and_then(|extension| extension.to_str()) == Some("bsol")
            {
                let (diagnostics, fixes) = collect_syntax_diagnostics(None, &uri, &text, None);
                let bsol_semantic_token_candidates = crate::session::lifecycle::bsol_semantic_token_candidates(&text);
                Document {
                    version: 0,
                    text,
                    syntax_definitions: Vec::new(),
                    syntax_hovers: Vec::new(),
                    syntax_symbols: Vec::new(),
                    bsol_semantic_token_candidates,
                    syntax_completion: None,
                    syntax_inlay_hints: Vec::new(),
                    syntax_documentation: Vec::new(),
                    syntax_diagnostics: diagnostics,
                    syntax_fixes: fixes,
                }
            } else {
                // Closed Beskid sources must use the same project-backed, full-closure
                // diagnostic path as open buffers. The structural helper deliberately
                // excludes lower-spine type checking, which made valid syntax with a
                // type error appear clean until the user opened the file.
                build_initial_workspace_document(state, &uri, 0, text).await
            };
        let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
        set_disk_snapshot(state, uri.clone(), doc).await;
        client.publish_diagnostics(uri, diagnostics, Some(0)).await;
    }

    signal_initial_scan_complete(state).await;

    rebuild_open_document_syntax_facts(state).await;

    let open_uris = {
        let s = state.read().await;
        s.docs
            .keys()
            .filter(|uri| uri_to_path(uri).is_some_and(|path| path.starts_with(root)))
            .cloned()
            .collect::<Vec<_>>()
    };
    for uri in open_uris {
        publish_diagnostics_for_uri(client, state, &uri).await;
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

/// Remove a workspace-indexed document and clear its diagnostics.
pub async fn clear_disk_snapshot(client: &Client, state: &RwLock<State>, uri: &Uri) {
    state.write().await.workspace_index.remove(uri);
    client.publish_diagnostics(uri.clone(), Vec::new(), None).await;
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
    path_to_uri(path).map(|u| u.to_string()).unwrap_or_else(|| format!("file://{}", path.display()))
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

/// Re-read changed paths on disk when buffers are closed; may invalidate compilation cache on manifest edits.
pub async fn refresh_after_disk_change(client: &Client, state: &RwLock<State>, changed_paths: &[PathBuf]) {
    if changed_paths
        .iter()
        .any(|p| p.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension) || is_lockfile_path(p))
    {
        invalidate_compilation_cache(state).await;
        rebuild_open_document_syntax_facts(state).await;
        let open_uris = { state.read().await.docs.keys().cloned().collect::<Vec<_>>() };
        for uri in open_uris {
            publish_diagnostics_for_uri(client, state, &uri).await;
        }
    }
    for path in changed_paths {
        if !path.extension().and_then(|ext| ext.to_str()).is_some_and(is_scannable_extension) {
            continue;
        }
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
        let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
        set_disk_snapshot(state, uri.clone(), doc).await;
        client.publish_diagnostics(uri, diagnostics, Some(0)).await;
    }
}

/// After `didClose`, reload disk contents into the workspace index when the file still exists.
pub async fn hydrate_disk_after_close(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let Some(path) = uri_to_path(uri) else {
        client.publish_diagnostics(uri.clone(), Vec::new(), None).await;
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
    let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
    set_disk_snapshot(state, uri.clone(), doc).await;
    client.publish_diagnostics(uri.clone(), diagnostics, Some(0)).await;
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Arc, Mutex},
    };

    use futures_util::StreamExt;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use tower_lsp_server::{Client, LspService, jsonrpc::Request, ls_types::PublishDiagnosticsParams};
    use tower_service::Service;

    use super::*;
    use crate::{server::backend::Backend, session::lifecycle::set_document};

    #[test]
    fn workspace_scan_includes_generic_bsol_without_regressing_existing_extensions() {
        for extension in ["bd", "bproj", "bws", "bsol"] {
            assert!(is_scannable_extension(extension), "{extension} must be scanned");
        }
        assert!(!is_scannable_extension("toml"));
    }

    async fn initialized_client() -> (Client, mpsc::UnboundedReceiver<PublishDiagnosticsParams>) {
        let captured = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&captured);
        let (mut service, mut socket) = LspService::new(move |client| {
            *slot.lock().expect("client slot") = Some(client.clone());
            Backend::new(client)
        });
        let client = captured.lock().expect("client slot").take().expect("captured client");
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(request) = socket.next().await {
                if request.method() == "textDocument/publishDiagnostics"
                    && let Some(params) = request.params()
                    && let Ok(published) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone())
                {
                    let _ = tx.send(published);
                }
            }
        });
        let init = Request::build("initialize")
            .params(serde_json::json!({"processId": null, "capabilities": {}}))
            .id(1)
            .finish();
        let response = service.call(init).await.expect("initialize service").expect("initialize response");
        assert!(response.error().is_none(), "{response:?}");
        service
            .call(Request::build("initialized").params(serde_json::json!({})).finish())
            .await
            .expect("initialized service");
        // The client owns the initialized connection after the request has completed.
        (client, rx)
    }

    async fn open_lock_fixture() -> (TempDir, RwLock<State>, Uri, PathBuf, String) {
        let temp = TempDir::new().expect("project");
        fs::create_dir_all(temp.path().join("Src")).expect("source directory");
        let manifest = temp.path().join("App.bproj");
        fs::write(
            &manifest,
            r#"App {
  name = "App"
  version = "0.1.0"
}

target "App" {
  kind = App
  entry = "Main.bd"
}
"#,
        )
        .expect("manifest");
        let source = "i32 Main() { return 0; }\n".to_string();
        let source_path = temp.path().join("Src/Main.bd");
        fs::write(&source_path, &source).expect("source");
        let plan = beskid_analysis::projects::build_compile_plan(&manifest, None).expect("plan");
        beskid_analysis::projects::prepare_project_workspace(&plan).expect("initial valid v2 lock");
        let lock = temp.path().join("Project.lock");
        let uri = path_to_uri(&source_path).expect("source URI");
        let state = RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        set_document(&state, uri.clone(), 1, source.clone()).await;
        (temp, state, uri, lock, source)
    }

    async fn next_for_uri(
        receiver: &mut mpsc::UnboundedReceiver<PublishDiagnosticsParams>,
        uri: &Uri,
    ) -> PublishDiagnosticsParams {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let published = receiver.recv().await.expect("client notification stream");
                if &published.uri == uri {
                    return published;
                }
            }
        })
        .await
        .expect("open source must receive publishDiagnostics without an edit")
    }

    #[tokio::test]
    async fn watched_lock_change_republishes_open_source_diagnostics_both_directions() {
        let (client, mut notifications) = initialized_client().await;
        let (_temp, state, uri, lock, _source) = open_lock_fixture().await;
        let valid = fs::read_to_string(&lock).expect("valid lock");

        fs::write(&lock, "# Project.lock v1\n").expect("replace lock with v1");
        refresh_after_disk_change(&client, &state, &[lock.clone()]).await;
        let invalid = next_for_uri(&mut notifications, &uri).await;
        assert!(invalid.diagnostics.iter().any(|d| d.message.contains("v1") && d.message.contains("beskid lock")));

        fs::write(&lock, valid).expect("restore v2");
        refresh_after_disk_change(&client, &state, &[lock]).await;
        let valid = next_for_uri(&mut notifications, &uri).await;
        assert!(!valid.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
    }

    #[tokio::test]
    async fn full_refresh_republishes_open_source_diagnostics_both_directions() {
        let (client, mut notifications) = initialized_client().await;
        let (temp, state, uri, lock, _source) = open_lock_fixture().await;
        let valid = fs::read_to_string(&lock).expect("valid lock");

        fs::write(&lock, "# Project.lock v1\n").expect("replace lock with v1");
        scan_workspace(&client, &state, temp.path(), None).await;
        let invalid = next_for_uri(&mut notifications, &uri).await;
        assert!(invalid.diagnostics.iter().any(|d| d.message.contains("v1") && d.message.contains("beskid lock")));

        fs::write(&lock, valid).expect("restore v2");
        scan_workspace(&client, &state, temp.path(), None).await;
        let valid = next_for_uri(&mut notifications, &uri).await;
        assert!(!valid.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
    }
}
