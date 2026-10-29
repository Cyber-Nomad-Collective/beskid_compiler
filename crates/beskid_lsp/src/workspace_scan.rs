//! Walk workspace roots to index `.bd` / `.bproj` / `.bws` / `.bsol` files and publish disk-backed diagnostics.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
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
            build_document, build_initial_workspace_document, index_project_documents, publish_diagnostics_for_uri,
            rebuild_open_document_syntax_facts, set_disk_snapshot,
        },
        project_context::invalidate_compilation_cache,
        startup::signal_initial_scan_complete,
        store::{Document, State},
    },
};

const MAX_CONCURRENT_READS: usize = 24;
/// Projects the workspace index types at once (see `State::index_permits`).
const MAX_CONCURRENT_INDEX: usize = 3;
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
pub async fn scan_workspace_root(
    client: &Client,
    state: &Arc<RwLock<State>>,
    root: &Path,
    focused_project: Option<&Path>,
) {
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

    let first_scan = !state.read().await.initial_scan_complete.load(std::sync::atomic::Ordering::Acquire);
    // Projects of open documents are analyzed first; every other project follows in the background.
    let open_roots: Vec<PathBuf> = {
        let open_paths: Vec<PathBuf> = state.read().await.docs.keys().filter_map(uri_to_path).collect();
        open_paths
            .iter()
            .filter_map(|path| {
                beskid_analysis::resolve_project_manifest_for_source_path(path, None)
                    .ok()
                    .flatten()
                    .and_then(|(manifest, _)| manifest.parent().map(Path::to_path_buf))
            })
            .collect()
    };
    let focus_root = focused_project.and_then(|manifest| manifest.parent());
    let is_manifest = |p: &Path| p.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension);
    paths.sort_by(|a, b| {
        let rank = |p: &Path| {
            (
                !is_manifest(p),
                !open_roots.iter().any(|root| p.starts_with(root)),
                !focus_root.is_some_and(|focus| p.starts_with(focus)),
            )
        };
        rank(a).cmp(&rank(b)).then_with(|| a.as_path().cmp(b.as_path()))
    });

    // A rescan keeps the current compilation context and facts, so requests are served from the
    // existing snapshot. Only the first scan, or a change in manifest/lock bytes since the last
    // look, invalidates and republishes.
    let authority_files: Vec<PathBuf> = paths
        .iter()
        .filter(|p| is_manifest(p))
        .flat_map(|manifest| {
            let lock = manifest.parent().map(|dir| dir.join(PROJECT_LOCK_FILE_NAME));
            std::iter::once(manifest.clone()).chain(lock.filter(|lock| lock.is_file()))
        })
        .collect();
    let observed_authority = observe_changes(state, &authority_files).await;
    let authority_changed = !observed_authority.paths.is_empty();
    let refresh = first_scan || authority_changed;
    if refresh {
        invalidate_compilation_cache(state).await;
        commit_observed(state, &observed_authority).await;
    }

    let total = paths.len() as u32;
    let mut last_emit = None;
    if total > 0 {
        maybe_emit_scan_progress(client, &mut last_emit, 0, total, Some(root.display().to_string())).await;
    }

    // Closed sources are indexed one project at a time: the project is typed once and every
    // file's facts come from that typing (see `index_project_documents`).
    let owners: std::collections::HashMap<PathBuf, PathBuf> = paths
        .iter()
        .filter(|path| !is_manifest(path) && path.extension().and_then(|e| e.to_str()) == Some("bd"))
        .filter_map(|path| {
            beskid_analysis::resolve_project_manifest_for_source_path(path, None)
                .ok()
                .flatten()
                .map(|(manifest, _)| (path.clone(), manifest))
        })
        .collect();
    let interactive = state.read().await.interactive_inflight.clone();
    let mut processed: u32 = 0;
    let sem = Semaphore::new(MAX_CONCURRENT_READS);
    let mut indexed_files: HashSet<PathBuf> = HashSet::new();
    for manifests_pass in [true, false] {
        if !manifests_pass {
            // Manifests are indexed: document requests stop waiting for the scan here, and the
            // closed-source analysis follows.
            signal_initial_scan_complete(state).await;

            // Projects in scan order (open documents' projects first), indexed a few at a time.
            let mut projects: Vec<PathBuf> = Vec::new();
            for path in &paths {
                if let Some(manifest) = owners.get(path)
                    && !projects.contains(manifest)
                {
                    projects.push(manifest.clone());
                }
            }
            // Each step is its own task: a step holds the database and blocking threads for a
            // whole project's typing, and must not stall the other steps or the state lock.
            let mut members_of: std::collections::HashMap<PathBuf, Vec<PathBuf>> = std::collections::HashMap::new();
            for path in &paths {
                if let Some(manifest) = owners.get(path) {
                    members_of.entry(manifest.clone()).or_default().push(path.clone());
                }
            }
            let mut steps = tokio::task::JoinSet::new();
            let mut pending = projects.into_iter();
            loop {
                while steps.len() < MAX_CONCURRENT_INDEX
                    && let Some(manifest) = pending.next()
                {
                    let members = members_of.remove(&manifest).unwrap_or_default();
                    let state = Arc::clone(state);
                    let client = client.clone();
                    steps.spawn(async move {
                        let mut group = Vec::new();
                        let mut unchanged = Vec::new();
                        let mut handled_on_close = Vec::new();
                        for member in &members {
                            let Some(uri) = uri_from_path(member) else { continue };
                            // An open document is analyzed as a buffer; closing it analyzes its disk text.
                            if state.read().await.docs.contains_key(&uri) {
                                handled_on_close.push(member.clone());
                                continue;
                            }
                            let Ok(text) = tokio::fs::read_to_string(member).await else { continue };
                            if !refresh
                                && state.read().await.workspace_index.get(&uri).is_some_and(|known| known.text == text)
                            {
                                unchanged.push(member.clone());
                                continue;
                            }
                            group.push((uri, member.clone(), text));
                        }
                        let started = Instant::now();
                        let mut documents = match index_project_documents(&state, &group).await {
                            Ok(documents) => documents,
                            Err(reason) => {
                                client
                                    .log_message(
                                        tower_lsp_server::ls_types::MessageType::WARNING,
                                        format!("indexing {} file by file: {reason}", manifest.display()),
                                    )
                                    .await;
                                Default::default()
                            }
                        };
                        client
                            .log_message(
                                tower_lsp_server::ls_types::MessageType::LOG,
                                format!(
                                    "indexed {}: {} of {} files in {} ms",
                                    manifest.display(),
                                    documents.len(),
                                    group.len(),
                                    started.elapsed().as_millis()
                                ),
                            )
                            .await;
                        let mut indexed = Vec::new();
                        for (uri, member, _) in group {
                            let Some(doc) = documents.remove(&uri) else { continue };
                            let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
                            set_disk_snapshot(&state, uri.clone(), doc).await;
                            client.publish_diagnostics(uri, diagnostics, Some(0)).await;
                            indexed.push(member);
                        }
                        (indexed, unchanged, handled_on_close)
                    });
                }
                let Some(finished) = steps.join_next().await else {
                    break;
                };
                // A step that panicked indexed nothing; its files are analyzed one by one below.
                let Ok((indexed, unchanged, handled_on_close)) = finished else {
                    continue;
                };
                indexed_files.extend(unchanged);
                indexed_files.extend(handled_on_close);
                processed = processed.saturating_add(indexed.len() as u32);
                indexed_files.extend(indexed);
                maybe_emit_scan_progress(client, &mut last_emit, processed, total, None).await;
            }
        }
        for path in paths.clone() {
            // Manifests first (structural and cheap); files of an indexed project are done; the
            // rest are analyzed one by one.
            if is_manifest(&path) != manifests_pass || indexed_files.contains(&path) {
                continue;
            }
            if !is_manifest(&path) {
                crate::scan_schedule::yield_to_interactive(&interactive).await;
            }
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
            if !refresh {
                let unchanged = state.read().await.workspace_index.get(&uri).is_some_and(|known| known.text == text);
                if unchanged {
                    continue;
                }
            }
            let doc =
                if is_manifest_extension(path.extension().and_then(|extension| extension.to_str()).unwrap_or_default())
                    || path.extension().and_then(|extension| extension.to_str()) == Some("bsol")
                {
                    let (diagnostics, fixes) = collect_syntax_diagnostics(None, &uri, &text, None);
                    let bsol_semantic_token_candidates =
                        crate::session::lifecycle::bsol_semantic_token_candidates(&text);
                    Document {
                        version: 0,
                        text,
                        syntax_definitions: Vec::new(),
                        syntax_hovers: Vec::new(),
                        syntax_symbols: Vec::new(),
                        bsol_semantic_token_candidates,
                        syntax_attribute_names: Vec::new(),
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
    }
    signal_initial_scan_complete(state).await;

    // A changed manifest or lock dropped the open documents' facts: analyze them again. The
    // first scan dropped nothing, and each open document's own analysis follows its open.
    if refresh && !first_scan {
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

/// Boundary of one root inside a multi-root scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootPhase {
    Started,
    Completed,
}

/// Called at each root boundary (logging, tests).
pub type RootHook = std::sync::Arc<
    dyn Fn(PathBuf, RootPhase) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync,
>;

/// Scan every workspace root in order. `workspace_index_complete` is set only after the last
/// root's closed sources have been analyzed, never between roots.
pub async fn scan_workspace_roots(
    client: &Client,
    state: &Arc<RwLock<State>>,
    roots: &[PathBuf],
    focused_project: Option<&Path>,
    hook: Option<RootHook>,
) {
    for root in roots {
        if let Some(hook) = &hook {
            hook(root.clone(), RootPhase::Started).await;
        }
        scan_workspace_root(client, state, root, focused_project).await;
        if let Some(hook) = &hook {
            hook(root.clone(), RootPhase::Completed).await;
        }
    }
    state.read().await.mark_workspace_index_complete();
}

/// Scan a single root as a complete workspace.
pub async fn scan_workspace(client: &Client, state: &Arc<RwLock<State>>, root: &Path, focused_project: Option<&Path>) {
    scan_workspace_roots(client, state, std::slice::from_ref(&root.to_path_buf()), focused_project, None).await;
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

async fn content_fingerprint(path: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(path).ok()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        Some(hasher.finish())
    })
    .await
    .ok()
    .flatten()
}

/// Watched-file events that can change analysis, plus the fingerprints that must be recorded
/// once they have been acted on.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ObservedChanges {
    pub paths: Vec<PathBuf>,
    /// `(path, fingerprint)`; `None` records that the file is gone.
    fingerprints: Vec<(PathBuf, Option<u64>)>,
}

/// Decide which events matter, without recording anything.
///
/// Events under `obj/`, `.beskid/` and `target/` (the server's own cache and materialization
/// output) are dropped. A manifest or lock whose bytes equal the last *committed* fingerprint
/// is dropped too (the server's own `Project.lock` sync). Observing never changes state, so the
/// outcome cannot depend on how many times or in what order callers look; the caller commits
/// with [`commit_observed`] after it has acted.
pub(crate) async fn observe_changes(state: &RwLock<State>, changed: &[PathBuf]) -> ObservedChanges {
    let mut observed = ObservedChanges::default();
    for path in changed {
        if crate::scan_schedule::is_ignored_watch_path(path) {
            continue;
        }
        let authority =
            path.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension) || is_lockfile_path(path);
        if authority {
            let fingerprint = content_fingerprint(path).await;
            let known = state.read().await.watched_fingerprints.get(path).copied();
            if known == fingerprint {
                // Equal bytes, or a file that was never seen and is still absent.
                continue;
            }
            observed.fingerprints.push((path.clone(), fingerprint));
        }
        observed.paths.push(path.clone());
    }
    observed
}

/// Record the fingerprints of changes that have been acted on.
pub(crate) async fn commit_observed(state: &RwLock<State>, observed: &ObservedChanges) {
    let mut write = state.write().await;
    for (path, fingerprint) in &observed.fingerprints {
        match fingerprint {
            Some(value) => {
                write.watched_fingerprints.insert(path.clone(), *value);
            }
            None => {
                write.watched_fingerprints.remove(path);
            }
        }
    }
}

/// What a watched-file event leaves for the caller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskRefresh {
    /// The changed files were handled here.
    Done,
    /// A manifest or lock changed: the caller runs a workspace scan, which observes the change
    /// again, types every project once for its closed files and republishes the open documents.
    ScanWorkspace,
}

/// React to files changed on disk. A closed source is analyzed again from its disk text. A
/// changed manifest or lock drops every fact derived under the old project authority at once
/// (requests fail closed until the scan rebuilds them) and asks for a workspace scan: the index
/// is the one path that types a project once for all its files, so a lock change costs one index
/// of the workspace, never one preparation per indexed file.
pub async fn refresh_after_disk_change(
    client: &Client,
    state: &RwLock<State>,
    changed_paths: &[PathBuf],
) -> DiskRefresh {
    let observed = observe_changes(state, changed_paths).await;
    let changed_paths = &observed.paths;
    let project_authority_changed = changed_paths
        .iter()
        .any(|p| p.extension().and_then(|e| e.to_str()).is_some_and(is_manifest_extension) || is_lockfile_path(p));
    if changed_paths.is_empty() {
        return DiskRefresh::Done;
    }
    let _span = crate::trace::Span::start("disk-refresh", || {
        format!("paths={} authority={project_authority_changed}", changed_paths.len())
    });
    if project_authority_changed {
        invalidate_compilation_cache(state).await;
        // The fingerprints stay uncommitted: the scan observes the same change and refreshes.
        return DiskRefresh::ScanWorkspace;
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
    commit_observed(state, &observed).await;
    DiskRefresh::Done
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
    let span = crate::trace::Span::start("hydrate", || format!("uri={}", uri.as_str()));
    let doc = build_document(state, uri, 0, text).await;
    drop(span);
    // Reopened while the disk text was analyzed: the open buffer owns the facts.
    if crate::features::formatting::open_texts::open_text(uri).is_some() {
        return;
    }
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

    #[tokio::test]
    async fn own_cache_writes_and_unchanged_lock_rewrites_cause_no_reaction() {
        let temp = TempDir::new().expect("project");
        let cache = temp.path().join("obj/beskid/cache/salsa");
        fs::create_dir_all(&cache).expect("cache dir");
        let salsa = cache.join("db.bin");
        fs::write(&salsa, b"x").expect("cache write");
        let materialized = temp.path().join("obj/beskid/materialized/Main.bd");
        let lock = temp.path().join("Project.lock");
        fs::write(&lock, "a").expect("lock");
        let state = RwLock::new(State::default());

        let first = observe_changes(&state, &[salsa.clone(), materialized, lock.clone()]).await;
        assert_eq!(first.paths, vec![lock.clone()], "the unseen lock is actionable, the obj/ events are not");
        commit_observed(&state, &first).await;

        // The server rewrites Project.lock with identical bytes: no reaction, no rescan.
        fs::write(&lock, "a").expect("same-content rewrite");
        assert!(observe_changes(&state, std::slice::from_ref(&lock)).await.paths.is_empty());
        assert!(observe_changes(&state, &[salsa]).await.paths.is_empty());

        fs::write(&lock, "b").expect("real edit");
        assert_eq!(observe_changes(&state, std::slice::from_ref(&lock)).await.paths, vec![lock]);
    }

    #[tokio::test]
    async fn observing_does_not_consume_the_change() {
        let temp = TempDir::new().expect("project");
        let lock = temp.path().join("Project.lock");
        fs::write(&lock, "a").expect("lock");
        let state = RwLock::new(State::default());
        // Two observers (for example a scan and a watched-file event) see the same change; neither
        // hides it from the other, so the outcome does not depend on who looked first.
        let scan_view = observe_changes(&state, std::slice::from_ref(&lock)).await;
        let event_view = observe_changes(&state, std::slice::from_ref(&lock)).await;
        assert_eq!(scan_view, event_view);
        assert_eq!(scan_view.paths, vec![lock.clone()]);
        // Only after the change was acted on and committed does it stop being news.
        commit_observed(&state, &scan_view).await;
        assert!(observe_changes(&state, std::slice::from_ref(&lock)).await.paths.is_empty());
        // A deleted lock is news once, then quiet.
        fs::remove_file(&lock).expect("remove");
        let gone = observe_changes(&state, std::slice::from_ref(&lock)).await;
        assert_eq!(gone.paths, vec![lock.clone()]);
        commit_observed(&state, &gone).await;
        assert!(observe_changes(&state, &[lock]).await.paths.is_empty());
    }

    #[tokio::test]
    async fn the_index_types_a_project_once_for_all_its_closed_files() {
        let (client, _notifications) = initialized_client().await;
        let root = TempDir::new().expect("root");
        write_project(root.path(), "Indexed");
        fs::write(root.path().join("Src/Util.bd"), "i32 Twice(i32 value) { return value + value; }\n")
            .expect("second source");
        fs::write(root.path().join("Src/Broken.bd"), "i32 Broken() { return missing_name; }\n").expect("third source");
        let state = Arc::new(RwLock::new(State::default()));
        scan_workspace_roots(&client, &state, &[root.path().to_path_buf()], None, None).await;

        let read = state.read().await;
        let document = |name: &str| {
            read.workspace_index
                .iter()
                .find(|(uri, _)| uri.as_str().ends_with(name))
                .map(|(_, document)| document.clone())
                .unwrap_or_else(|| panic!("{name} is indexed"))
        };
        assert!(document("Util.bd").syntax_documentation.iter().any(|fact| fact.name == "Twice"));
        assert!(document("Main.bd").syntax_diagnostics.is_empty(), "{:?}", document("Main.bd").syntax_diagnostics);
        assert!(
            !document("Broken.bd").syntax_diagnostics.is_empty(),
            "the error of one file is reported against that file"
        );
        assert_eq!(read.databases.analyses(root.path()), 1, "one typing of the project serves all three files");
    }

    #[tokio::test]
    async fn fingerprinting_does_not_block_the_async_runtime() {
        let temp = TempDir::new().expect("project");
        let lock = temp.path().join("Project.lock");
        fs::write(&lock, vec![b'x'; 8 * 1024 * 1024]).expect("large lock");
        let ticks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ticker = tokio::spawn({
            let ticks = Arc::clone(&ticks);
            async move {
                loop {
                    ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tokio::task::yield_now().await;
                }
            }
        });
        for _ in 0..20 {
            assert!(content_fingerprint(&lock).await.is_some());
        }
        ticker.abort();
        assert!(ticks.load(std::sync::atomic::Ordering::Relaxed) > 0, "other tasks ran while files were read");
    }

    fn write_project(root: &Path, name: &str) {
        fs::create_dir_all(root.join("Src")).expect("src");
        fs::write(
            root.join(format!("{name}.bproj")),
            format!("{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n"),
        )
        .expect("manifest");
        fs::write(root.join("Src/Main.bd"), "i32 Main() { return 0; }\n").expect("source");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn index_completes_only_after_every_root_is_analyzed() {
        let (client, _notifications) = initialized_client().await;
        let first = TempDir::new().expect("first root");
        let second = TempDir::new().expect("second root");
        write_project(first.path(), "First");
        write_project(second.path(), "Second");
        let state = Arc::new(RwLock::new(State::default()));
        let complete = Arc::clone(&state.read().await.workspace_index_complete);
        let seen: Arc<Mutex<Vec<(String, bool)>>> = Arc::new(Mutex::new(Vec::new()));
        let hook: RootHook = {
            let seen = Arc::clone(&seen);
            let complete = Arc::clone(&complete);
            Arc::new(move |root, phase| {
                let seen = Arc::clone(&seen);
                let flag = complete.load(std::sync::atomic::Ordering::Acquire);
                Box::pin(async move {
                    seen.lock()
                        .expect("seen")
                        .push((format!("{:?}:{}", phase, root.file_name().unwrap().to_string_lossy()), flag));
                })
            })
        };
        let roots = vec![first.path().to_path_buf(), second.path().to_path_buf()];
        scan_workspace_roots(&client, &state, &roots, None, Some(hook)).await;

        let seen = seen.lock().expect("seen").clone();
        assert_eq!(seen.len(), 4, "two boundaries per root: {seen:?}");
        assert!(seen.iter().all(|(_, flag)| !flag), "the index is partial at every root boundary: {seen:?}");
        let read = state.read().await;
        assert!(read.workspace_index_complete.load(std::sync::atomic::Ordering::Acquire));
        let indexed = |name: &str| read.workspace_index.keys().any(|uri| uri.as_str().contains(name));
        assert!(indexed("First") && indexed("Second"), "both roots' closed sources are indexed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn startup_barrier_opens_before_closed_sources_and_the_index_completes_after() {
        let (client, _notifications) = initialized_client().await;
        let temp = TempDir::new().expect("project");
        fs::create_dir_all(temp.path().join("Src")).expect("src");
        fs::write(
            temp.path().join("App.bproj"),
            "App {\n  name = \"App\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
        )
        .expect("manifest");
        fs::write(temp.path().join("Src/Main.bd"), "i32 Main() { return 0; }\n").expect("source");
        let state = Arc::new(RwLock::new(State::default()));
        // An interactive request holds the scan back at the first closed source.
        let gate = Arc::clone(&state.read().await.interactive_inflight);
        let request = crate::scan_schedule::InteractiveGuard::new(&gate);
        let scan = tokio::spawn({
            let state = Arc::clone(&state);
            let client = client.clone();
            let root = temp.path().to_path_buf();
            async move { scan_workspace(&client, &state, &root, None).await }
        });
        tokio::time::timeout(Duration::from_secs(5), crate::session::startup::wait_for_initial_scan(&state))
            .await
            .expect("requests are released once manifests are indexed");
        {
            let read = state.read().await;
            assert!(
                !read.workspace_index_complete.load(std::sync::atomic::Ordering::Acquire),
                "closed sources are not analyzed yet: the index is partial"
            );
            assert!(
                read.workspace_index.keys().all(|uri| !uri.as_str().ends_with(".bd")),
                "no closed source indexed while the scan yields"
            );
            assert!(
                read.workspace_index.keys().any(|uri| uri.as_str().ends_with("App.bproj")),
                "the manifest is already indexed"
            );
        }
        drop(request);
        tokio::time::timeout(Duration::from_secs(60), scan).await.expect("scan ends").expect("scan task");
        let read = state.read().await;
        assert!(read.workspace_index_complete.load(std::sync::atomic::Ordering::Acquire));
        assert!(read.workspace_index.values().any(|doc| doc.text.contains("Main")), "closed source indexed");
    }

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
            Backend::new(client, Arc::new(crate::crash::CrashReporter::new(None)))
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

    async fn open_lock_fixture() -> (TempDir, Arc<RwLock<State>>, Uri, PathBuf, String) {
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
        beskid_analysis::projects::prepare_project_workspace(&plan).expect("initial valid lock");
        let lock = temp.path().join("Project.lock");
        let uri = path_to_uri(&source_path).expect("source URI");
        let state = Arc::new(RwLock::new(State::default()));
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

    /// A watched-file event as the backend handles it: the disk refresh, then the workspace
    /// scan it asks for after a manifest or lock change.
    async fn watched_change(client: &Client, state: &Arc<RwLock<State>>, root: &Path, path: &Path) {
        if refresh_after_disk_change(client, state, std::slice::from_ref(&path.to_path_buf())).await
            == DiskRefresh::ScanWorkspace
        {
            scan_workspace(client, state, root, None).await;
        }
    }

    #[tokio::test]
    async fn a_changed_lock_asks_for_one_workspace_scan_and_drops_the_old_facts_at_once() {
        let (client, _notifications) = initialized_client().await;
        let (_temp, state, uri, lock, _source) = open_lock_fixture().await;
        assert!(!state.read().await.docs[&uri].syntax_documentation.is_empty(), "the open source has facts");
        let source_path = uri_to_path(&uri).expect("path");
        assert_eq!(refresh_after_disk_change(&client, &state, &[source_path]).await, DiskRefresh::Done);

        fs::write(&lock, "# Project.lock v1\n").expect("replace the lock with an older format");
        assert_eq!(
            refresh_after_disk_change(&client, &state, std::slice::from_ref(&lock)).await,
            DiskRefresh::ScanWorkspace
        );
        assert!(state.read().await.docs[&uri].syntax_documentation.is_empty(), "facts of the old authority are gone");
        // The change is left for the scan to observe, so the scan refreshes every project.
        assert_eq!(observe_changes(&state, std::slice::from_ref(&lock)).await.paths, vec![lock]);
    }

    #[tokio::test]
    async fn watched_lock_change_republishes_open_source_diagnostics_both_directions() {
        let (client, mut notifications) = initialized_client().await;
        let (temp, state, uri, lock, _source) = open_lock_fixture().await;
        let valid = fs::read_to_string(&lock).expect("valid lock");

        fs::write(&lock, "# Project.lock v1\n").expect("replace the lock with an older format");
        watched_change(&client, &state, temp.path(), &lock).await;
        let invalid = next_for_uri(&mut notifications, &uri).await;
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unsupported format") && d.message.contains("beskid dev project lock"))
        );

        fs::write(&lock, valid).expect("restore the valid lock");
        watched_change(&client, &state, temp.path(), &lock).await;
        let valid = next_for_uri(&mut notifications, &uri).await;
        assert!(!valid.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
    }

    #[tokio::test]
    async fn watched_lock_change_republishes_closed_source_diagnostics_both_directions() {
        let (client, mut notifications) = initialized_client().await;
        let (temp, state, uri, lock, _source) = open_lock_fixture().await;
        state.write().await.docs.remove(&uri);
        scan_workspace(&client, &state, temp.path(), None).await;
        let initial = next_for_uri(&mut notifications, &uri).await;
        assert!(!initial.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
        assert!(state.read().await.workspace_index.contains_key(&uri));
        let valid = fs::read_to_string(&lock).expect("valid lock");

        fs::write(&lock, "# Project.lock v1\n").expect("replace the lock with an older format");
        watched_change(&client, &state, temp.path(), &lock).await;
        let invalid = next_for_uri(&mut notifications, &uri).await;
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unsupported format") && d.message.contains("beskid dev project lock"))
        );

        fs::write(&lock, valid).expect("restore the valid lock");
        watched_change(&client, &state, temp.path(), &lock).await;
        let valid = next_for_uri(&mut notifications, &uri).await;
        assert!(!valid.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
    }

    #[tokio::test]
    async fn full_refresh_republishes_open_source_diagnostics_both_directions() {
        let (client, mut notifications) = initialized_client().await;
        let (temp, state, uri, lock, _source) = open_lock_fixture().await;
        let valid = fs::read_to_string(&lock).expect("valid lock");

        fs::write(&lock, "# Project.lock v1\n").expect("replace the lock with an older format");
        scan_workspace(&client, &state, temp.path(), None).await;
        let invalid = next_for_uri(&mut notifications, &uri).await;
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unsupported format") && d.message.contains("beskid dev project lock"))
        );

        fs::write(&lock, valid).expect("restore the valid lock");
        scan_workspace(&client, &state, temp.path(), None).await;
        let valid = next_for_uri(&mut notifications, &uri).await;
        assert!(!valid.diagnostics.iter().any(|d| d.message.contains("Project.lock")));
    }
}
