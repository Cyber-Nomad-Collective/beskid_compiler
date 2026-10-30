use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::commands::{
    PckgRegistryState, focused_project_from_configuration, focused_project_from_value, handle_project_explorer_command,
    pckg_registry, symbol_documentation,
};
use crate::crash::CrashReporter;
use crate::features::{
    code_actions, completion, definition, document_symbols, formatting, hover, inlay_hints, references, rename,
    semantic_tokens, signature_help, workspace_symbols,
};
use crate::logging::{ClientLogFilter, client_log};
use crate::protocol::refresh::{FactsRefresh, RefreshKind};
use crate::protocol::request::{
    CurrentDocument, EDIT_FACTS_WAIT, REFRESH_FACTS_WAIT, RequestFacts, STALE_SAFE_FACTS_WAIT, content_modified,
    document_facts, latest_document, wait_for_current_facts,
};
use crate::scan_schedule::{InteractiveGuard, ScanCoordinator, ScanRunGuard};
use crate::server::init::initialize_result;
use crate::session::db_access::{parallel_project_db, run_blocking_section, with_shared_project_db};
use crate::session::lifecycle::{analyze_open_document, publish_diagnostics_for_uri, remove_document};
use crate::session::mod_host::mod_host_from_initialization_options;
use crate::session::store::State;
use crate::workspace_scan::{
    DiskRefresh, clear_closed_workspace_under_root, hydrate_disk_after_close, refresh_after_disk_change,
    scan_workspace_roots, uri_to_path,
};

/// Longest completion waits for the analysis of the newest text before it answers from the
/// last facts carried over the edit.
const COMPLETION_FACTS_WAIT: Duration = Duration::from_millis(100);
/// Longest completion waits for its project's database, busy with another analysis.
const COMPLETION_DB_WAIT: Duration = Duration::from_millis(100);
/// Longest completion's database queries run before the owned facts answer instead.
const COMPLETION_QUERY_LIMIT: Duration = Duration::from_millis(1000);

/// Tower-LSP [`LanguageServer`] implementation for Beskid (sync, diagnostics, IDE features).
pub struct Backend {
    client: Client,
    state: Arc<RwLock<State>>,
    workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    pckg_registry: Arc<RwLock<PckgRegistryState>>,
    log_filter: Arc<RwLock<ClientLogFilter>>,
    diagnostics_revision: Arc<Mutex<HashMap<Uri, u64>>>,
    reporter: Arc<CrashReporter>,
    scan: Arc<ScanCoordinator>,
    interactive: Arc<crate::scan_schedule::InteractiveGate>,
    token_refresh: Arc<FactsRefresh>,
    hint_refresh: Arc<FactsRefresh>,
}

impl Backend {
    /// Construct server state wired to the LSP [`Client`] (empty workspace until `initialize`).
    pub fn new(client: Client, reporter: Arc<CrashReporter>) -> Self {
        let state = State::default();
        let interactive = Arc::clone(&state.interactive_inflight);
        Self {
            client,
            state: Arc::new(RwLock::new(state)),
            workspace_roots: Arc::new(RwLock::new(Vec::new())),
            pckg_registry: Arc::new(RwLock::new(PckgRegistryState::default())),
            log_filter: Arc::new(RwLock::new(ClientLogFilter::Info)),
            diagnostics_revision: Arc::new(Mutex::new(HashMap::new())),
            reporter,
            scan: Arc::new(ScanCoordinator::default()),
            interactive,
            token_refresh: Arc::new(FactsRefresh::new(RefreshKind::SemanticTokens)),
            hint_refresh: Arc::new(FactsRefresh::new(RefreshKind::InlayHints)),
        }
    }

    /// Unblock analysis paths that wait on the initial workspace scan (integration tests only).
    pub async fn signal_workspace_ready_for_tests(&self) {
        crate::session::startup::signal_initial_scan_complete(&self.state).await;
    }

    /// Opens a document as the stdio transport does: the transport records the open text, which
    /// analysis and requests compare against, before `did_open` runs (integration tests that call
    /// the backend directly only).
    pub async fn did_open_for_tests(&self, params: DidOpenTextDocumentParams) {
        observe_text_sync_for_tests("textDocument/didOpen", &params);
        self.did_open(params).await;
    }

    /// Closes a document as the stdio transport does (integration tests only).
    pub async fn did_close_for_tests(&self, params: DidCloseTextDocumentParams) {
        observe_text_sync_for_tests("textDocument/didClose", &params);
        self.did_close(params).await;
    }

    async fn schedule_publish_diagnostics(&self, uri: Uri) {
        self.publisher().await.schedule(uri).await;
    }

    async fn publisher(&self) -> DiagnosticsPublisher {
        DiagnosticsPublisher {
            client: self.client.clone(),
            state: Arc::clone(&self.state),
            revisions: Arc::clone(&self.diagnostics_revision),
            filter: *self.log_filter.read().await,
            reporter: Arc::clone(&self.reporter),
        }
    }

    /// Analyze the newest text of an open document in the background, then publish its
    /// diagnostics. The notification returns at once, so the transport and requests on other
    /// documents never wait for the analysis; requests that need this document's newest facts
    /// wait for them on their own (see `protocol::request`).
    async fn analyze_in_background(&self, uri: Uri, version: i32, text: String) {
        let state = Arc::clone(&self.state);
        let publisher = self.publisher().await;
        let interactive = Arc::clone(&self.interactive);
        self.reporter.spawn_guarded("beskid/analyzeDocument", Some(uri.as_str().to_string()), async move {
            let _active = InteractiveGuard::new(&interactive);
            if analyze_open_document(&state, uri.clone(), version, text).await {
                publisher.schedule(uri).await;
            }
        });
    }

    /// After an answer from carried facts (semantic tokens, inlay hints): wait in the
    /// background for the facts of the newest text of `uri` (at most [`REFRESH_FACTS_WAIT`]),
    /// then send one refresh request of that kind (only to a client that supports it; one
    /// waiting refresh of each kind per document).
    fn refresh_when_current(&self, refresh: &Arc<FactsRefresh>, uri: &Uri) {
        if !refresh.claim(uri) {
            return;
        }
        let state = Arc::clone(&self.state);
        let refresh = Arc::clone(refresh);
        let client = self.client.clone();
        let uri = uri.clone();
        let task = match refresh.kind() {
            RefreshKind::SemanticTokens => "beskid/semanticTokensRefresh",
            RefreshKind::InlayHints => "beskid/inlayHintRefresh",
        };
        self.reporter.spawn_guarded(task, Some(uri.as_str().to_string()), async move {
            // A burst longer than the limit gets a refresh anyway: the client then answers from
            // the facts stored by now, and a further stale answer claims the next refresh.
            wait_for_current_facts(&state, &uri, REFRESH_FACTS_WAIT).await;
            refresh.release(&uri);
            let sent = match refresh.kind() {
                RefreshKind::SemanticTokens => client.semantic_tokens_refresh().await,
                RefreshKind::InlayHints => client.inlay_hint_refresh().await,
            };
            if let Err(error) = sent {
                tracing::debug!(%error, "refresh request failed");
            }
        });
    }

    /// The facts of the newest text for an edit-producing request, within [`EDIT_FACTS_WAIT`];
    /// `ContentModified` when the analysis of the newest text has not finished by then.
    async fn edit_facts(&self, uri: &Uri) -> Result<Option<CurrentDocument>> {
        let facts = document_facts(&self.state, uri, EDIT_FACTS_WAIT).await.edits(uri);
        if facts.is_err() {
            tracing::debug!(uri = uri.as_str(), "edit request answered ContentModified: newest text not analyzed");
        }
        facts
    }

    /// Request a workspace scan. Never waits for the scan: at most one scan runs at a time,
    /// and triggers that arrive while it runs coalesce into a single follow-up round.
    async fn refresh_workspace_scan(&self) {
        if !self.scan.request() {
            return;
        }
        let scan = Arc::clone(&self.scan);
        let client = self.client.clone();
        let state = Arc::clone(&self.state);
        let roots = Arc::clone(&self.workspace_roots);
        let log_filter = Arc::clone(&self.log_filter);
        self.reporter.spawn_guarded("beskid/workspaceScan", None, async move {
            let mut run = ScanRunGuard::new(Arc::clone(&scan));
            loop {
                let current_roots = roots.read().await.clone();
                let focused = state.read().await.focused_project.clone();
                let filter = *log_filter.read().await;
                let hook_client = client.clone();
                let hook: crate::workspace_scan::RootHook = Arc::new(move |root, phase| {
                    let client = hook_client.clone();
                    Box::pin(async move {
                        let what = match phase {
                            crate::workspace_scan::RootPhase::Started => "started",
                            crate::workspace_scan::RootPhase::Completed => "completed",
                        };
                        client_log(
                            &client,
                            filter,
                            MessageType::INFO,
                            format!("workspace scan {what}: {}", root.display()),
                        )
                        .await;
                    })
                });
                scan_workspace_roots(&client, &state, &current_roots, focused.as_deref(), Some(hook)).await;
                if !scan.finish_round() {
                    run.disarm();
                    break;
                }
            }
        });
    }
}

/// Debounced, per-document diagnostics publication.
#[derive(Clone)]
struct DiagnosticsPublisher {
    client: Client,
    state: Arc<RwLock<State>>,
    revisions: Arc<Mutex<HashMap<Uri, u64>>>,
    filter: ClientLogFilter,
    reporter: Arc<CrashReporter>,
}

impl DiagnosticsPublisher {
    async fn schedule(&self, uri: Uri) {
        let rev = {
            let mut map = self.revisions.lock().await;
            let next = map.get(&uri).copied().unwrap_or(0).saturating_add(1);
            map.insert(uri.clone(), next);
            next
        };
        let this = self.clone();
        // A panic while analysing must not silently kill diagnostics for the session.
        self.reporter.spawn_guarded("textDocument/publishDiagnostics", Some(uri.as_str().to_string()), async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            let should_run = {
                let map = this.revisions.lock().await;
                map.get(&uri).copied() == Some(rev)
            };
            if should_run {
                client_log(
                    &this.client,
                    this.filter,
                    MessageType::LOG,
                    format!("publishing diagnostics for {}", uri.as_str()),
                )
                .await;
                publish_diagnostics_for_uri(&this.client, &this.state, &uri).await;
            }
        });
    }
}

/// `ContentModified` when the client sent an edit while an edit-producing request computed its
/// answer from `current`: the edits would apply to a text the client no longer has.
fn unchanged_since(uri: &Uri, current: &CurrentDocument) -> Result<()> {
    if current.still_current(uri) { Ok(()) } else { Err(content_modified()) }
}

/// An empty completion list the client must request again (the answer ran past its limit).
fn incomplete_completion() -> CompletionResponse {
    CompletionResponse::List(CompletionList { is_incomplete: true, items: Vec::new() })
}

fn observe_text_sync_for_tests(method: &'static str, params: &impl serde::Serialize) {
    let params = serde_json::to_value(params).expect("text sync parameters serialize");
    let request = tower_lsp_server::jsonrpc::Request::build(method).params(params).finish();
    formatting::open_texts::observe(&request);
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.token_refresh.configure(&params.capabilities);
        self.hint_refresh.configure(&params.capabilities);
        let mut roots: Vec<PathBuf> = params
            .workspace_folders
            .unwrap_or_default()
            .into_iter()
            .filter_map(|folder| uri_to_path(&folder.uri))
            .collect();
        if roots.is_empty() {
            // Legacy clients may send only deprecated `root_uri` when workspace folders are absent.
            #[allow(deprecated)]
            let legacy_root = params.root_uri.as_ref().and_then(uri_to_path);
            if let Some(path) = legacy_root {
                roots.push(path);
            }
        }
        *self.workspace_roots.write().await = roots;

        if let Some(options) = params.initialization_options {
            // Validate the Mod host first: an invalid selection fails initialization rather than
            // silently running Mods in the installed toolchain.
            if let Some(mod_host) = mod_host_from_initialization_options(&options).map_err(|error| {
                tower_lsp_server::jsonrpc::Error::invalid_params(format!("invalid `modHost` option: {error:#}"))
            })? {
                self.state.write().await.mod_host = mod_host;
            }
            if let Some(level) = options.get("logLevel").and_then(|v| v.as_str()) {
                *self.log_filter.write().await = ClientLogFilter::parse(level);
            }
            if let Some(focused) = focused_project_from_value(&options) {
                self.state.write().await.focused_project = Some(focused);
            }
        }
        Ok(initialize_result())
    }

    async fn initialized(&self, _: InitializedParams) {
        let filter = *self.log_filter.read().await;
        client_log(&self.client, filter, MessageType::INFO, "Beskid LSP initialized".to_string()).await;
        // Never awaits the scan itself: the scan runs as a crash-guarded background task, so a
        // large workspace does not hold a transport slot and delay formatting.
        self.refresh_workspace_scan().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.analyze_in_background(doc.uri, doc.version, doc.text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // The transport applied the changes to the open text in input order already.
        let Some(text) = formatting::open_texts::open_text(&uri) else {
            return;
        };
        self.analyze_in_background(uri, params.text_document.version, text).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.schedule_publish_diagnostics(params.text_document.uri.clone()).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        remove_document(&self.state, &uri).await;
        // Analyzing the disk text of the closed file is background work: a reopen of the same
        // document must not wait for it.
        let client = self.client.clone();
        let state = Arc::clone(&self.state);
        self.reporter.spawn_guarded("beskid/hydrateClosedDocument", Some(uri.as_str().to_string()), async move {
            hydrate_disk_after_close(&client, &state, &uri).await;
        });
    }

    // Hover answers at once from the last analyzed facts, carried over the edits whose
    // analysis is still running.
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params;
        let uri = position.text_document.uri;
        let Some(document) = latest_document(&self.state, &uri).await else {
            return Ok(None);
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        Ok(hover::handler::handle_hover(&uri, &document, offset))
    }

    // Definition answers at once, like hover, from the last analyzed facts carried over the
    // edits whose analysis is still running.
    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params;
        let uri = position.text_document.uri;
        let Some(document) = latest_document(&self.state, &uri).await else {
            return Ok(None);
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        Ok(definition::handler::handle_definition(&uri, &document, offset))
    }

    async fn goto_declaration(
        &self,
        params: request::GotoDeclarationParams,
    ) -> Result<Option<request::GotoDeclarationResponse>> {
        let position = params.text_document_position_params;
        let uri = position.text_document.uri;
        let Some(document) = latest_document(&self.state, &uri).await else {
            return Ok(None);
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        Ok(definition::handler::handle_definition(&uri, &document, offset))
    }

    // References are read-only: past a short budget they come from the last analyzed facts
    // carried over to the newest text, like hover.
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let include_declaration = params.context.include_declaration;
        let position = params.text_document_position;
        let uri = position.text_document.uri;
        let Some(document) = document_facts(&self.state, &uri, STALE_SAFE_FACTS_WAIT).await.stale_safe() else {
            return Ok(Some(Vec::new()));
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        let entry_path = uri_to_path(&uri);
        Ok(Some(references::handler::handle_references(
            &uri,
            &document,
            offset,
            include_declaration,
            entry_path.as_deref(),
        )))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let _active = InteractiveGuard::new(&self.interactive);
        let position = params.text_document_position;
        let uri = position.text_document.uri;
        // The facts of the newest text when its analysis finishes within the budget; otherwise
        // the last facts carried over the edit, answered without database queries.
        let (document, current) = match document_facts(&self.state, &uri, COMPLETION_FACTS_WAIT).await {
            RequestFacts::Current(current) => (current.document, true),
            RequestFacts::Missing => return Ok(Some(CompletionResponse::Array(Vec::new()))),
            RequestFacts::Carried(carried) => (carried, false),
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        // `use` paths complete from the module layout of the effective roots, with no analysis
        // facts and no database access, so they answer while a scan or a prepare is running.
        if completion::use_path::use_path_context(&document.text, offset).is_some() {
            let roots = match uri_to_path(&uri) {
                Some(path) => crate::session::project_context::effective_roots_for_path(&self.state, &path).await,
                None => None,
            };
            let items = roots
                .and_then(|roots| completion::use_path::use_path_items(&document.text, offset, &roots))
                .unwrap_or_default();
            return Ok(Some(CompletionResponse::Array(items)));
        }
        // The completion anchor belongs to the database of the document's project. While that
        // database is busy with an analysis, the owned facts answer.
        let db = if current {
            let project_root = match uri_to_path(&uri) {
                Some(path) => crate::session::project_context::cached_compilation_context(&self.state, &path)
                    .await
                    .and_then(|context| context.compile_plan.map(|plan| plan.project_root)),
                None => None,
            };
            tokio::time::timeout(COMPLETION_DB_WAIT, parallel_project_db(&self.state, project_root.as_deref()))
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        let owned_document = db.is_some().then(|| document.clone());
        let cancel = db.as_ref().map(|db| salsa::Database::cancellation_token(db));
        let query_uri = uri.clone();
        let query = tokio::task::spawn_blocking(move || {
            crate::crash::run_blocking_recorded(|| {
                salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    completion::handler::handle_completion(db.as_ref(), &query_uri, &document, offset)
                }))
            })
        });
        // A database query that runs long (a large module's members) is cancelled and the owned
        // facts answer; the cancelled query releases its handle on the project's storage. The
        // owned-facts answer runs on a blocking thread too, never inline on the async worker, and
        // has the same limit; past it (and past the limit of a request without a database) the
        // answer is an empty incomplete list, so the client asks again on the next keystroke.
        let outcome = match tokio::time::timeout(COMPLETION_QUERY_LIMIT, query).await {
            Ok(outcome) => outcome,
            Err(_) => {
                if let Some(cancel) = cancel {
                    cancel.cancel();
                }
                let Some(owned) = owned_document else {
                    tracing::debug!(uri = uri.as_str(), "completion without a database exceeded its limit");
                    return Ok(Some(incomplete_completion()));
                };
                let fallback = tokio::task::spawn_blocking(move || {
                    crate::crash::run_blocking_recorded(|| {
                        completion::handler::handle_completion(None, &uri, &owned, offset)
                    })
                });
                return match tokio::time::timeout(COMPLETION_QUERY_LIMIT, fallback).await {
                    // Re-raise a worker panic on the request task so the service-level guard reports it.
                    Ok(Ok(response)) => Ok(Some(response.unwrap_or_else(|panic| panic.resume()))),
                    Ok(Err(error)) => {
                        tracing::error!(%error, "owned-facts completion task failed");
                        Err(tower_lsp_server::jsonrpc::Error::internal_error())
                    }
                    Err(_) => Ok(Some(incomplete_completion())),
                };
            }
        };
        let response = outcome
            .map_err(|error| {
                tracing::error!(%error, "parallel Salsa completion task failed");
                tower_lsp_server::jsonrpc::Error::internal_error()
            })?
            // Re-raise a worker panic on the request task so the service-level guard reports it.
            .unwrap_or_else(|panic| panic.resume())
            .map_err(|cancelled| {
                tracing::debug!(%cancelled, "parallel Salsa completion invalidated by a newer input revision");
                tower_lsp_server::jsonrpc::Error::content_modified()
            })?;
        Ok(Some(response))
    }

    // The outline is syntax-only: it reads the open text (current in transport order) and does
    // not wait for the analysis of an earlier edit. A text that does not parse is outlined from
    // the last analyzed symbols carried over the edit, so the outline never runs a recovery.
    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let text = match formatting::open_texts::open_text(&uri) {
            Some(text) => text,
            None => match latest_document(&self.state, &uri).await {
                Some(document) => document.text,
                None => return Ok(Some(DocumentSymbolResponse::Nested(Vec::new()))),
            },
        };
        let parsed = run_blocking_section(|| document_symbols::handler::document_symbols_for_text(&uri, &text));
        if let Some(outline) = parsed {
            return Ok(Some(outline));
        }
        let Some(document) = latest_document(&self.state, &uri).await else {
            return Ok(Some(DocumentSymbolResponse::Nested(Vec::new())));
        };
        Ok(Some(document_symbols::handler::outline(&document.text, &document.syntax_symbols)))
    }

    // Tokens never wait for the analysis of a burst of edits: past a short budget they come
    // from the last analyzed facts carried over to the newest text (no tokens when the buffer
    // has no facts yet), and a client that supports it is asked to refresh once the current
    // facts are stored.
    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let document = match document_facts(&self.state, &uri, STALE_SAFE_FACTS_WAIT).await {
            RequestFacts::Current(current) => current.document,
            RequestFacts::Missing => {
                self.refresh_when_current(&self.token_refresh, &uri);
                return Ok(None);
            }
            RequestFacts::Carried(carried) => {
                self.refresh_when_current(&self.token_refresh, &uri);
                carried
            }
        };
        Ok(Some(semantic_tokens::handler::handle_semantic_tokens(&uri, &document)))
    }

    // Inlay hints answer like semantic tokens: past a short budget from the hints carried over
    // the edit (none inside the edited region), and a client that supports it is asked with
    // `workspace/inlayHint/refresh` to request them again once the current facts are stored.
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.clone();
        let document = match document_facts(&self.state, &uri, STALE_SAFE_FACTS_WAIT).await {
            RequestFacts::Current(current) => current.document,
            RequestFacts::Missing => {
                self.refresh_when_current(&self.hint_refresh, &uri);
                return Ok(Some(Vec::new()));
            }
            RequestFacts::Carried(carried) => {
                self.refresh_when_current(&self.hint_refresh, &uri);
                carried
            }
        };
        Ok(Some(inlay_hints::handler::handle_inlay_hints(&uri, &document, &params)))
    }

    // Signature help is read-only and answers from carried facts past a short budget.
    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let position = params.text_document_position_params;
        let uri = position.text_document.uri;
        let Some(document) = document_facts(&self.state, &uri, STALE_SAFE_FACTS_WAIT).await.stale_safe() else {
            return Ok(None);
        };
        let offset = crate::position::position_to_offset(&document.text, position.position);
        Ok(signature_help::handler::handle_signature_help(&uri, &document, offset))
    }

    // Code actions carry edits: they come only from the facts of the newest text. Past a short
    // budget, or when the client edits while they are computed, the answer is `ContentModified`
    // and the client asks again.
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let Some(current) = self.edit_facts(&uri).await? else {
            return Ok(Some(Vec::new()));
        };
        let actions = code_actions::handler::handle_code_actions(&uri, &current.document, &params);
        unchanged_since(&uri, &current)?;
        Ok(Some(actions))
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        let state = self.state.read().await;
        Ok(Some(workspace_symbols::handler::handle_workspace_symbols(&state, params)))
    }

    // Prepare rename starts an edit: its range comes only from the facts of the newest text, so
    // it matches the rename that follows. Otherwise `ContentModified`, like rename.
    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let Some(current) = self.edit_facts(&uri).await? else {
            return Ok(None);
        };
        let offset = crate::position::position_to_offset(&current.document.text, params.position);
        let range = rename::handler::handle_prepare_rename(&uri, &current.document, offset);
        unchanged_since(&uri, &current)?;
        Ok(range)
    }

    // Rename edits come only from the facts of the newest text: a rename from carried facts
    // would miss the occurrences inside the edited region. Past a short budget, or when the
    // client edits while the edits are computed, the answer is `ContentModified`.
    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let new_name = params.new_name;
        let request = params.text_document_position;
        let uri = request.text_document.uri;
        let Some(current) = self.edit_facts(&uri).await? else {
            return Ok(None);
        };
        let edit = rename::handler::handle_rename(&uri, &current.document, request.position, &new_name);
        unchanged_since(&uri, &current)?;
        Ok(edit)
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<LSPAny>> {
        let _active = InteractiveGuard::new(&self.interactive);
        if params.command == "beskid.refreshWorkspace" {
            self.refresh_workspace_scan().await;
            return Ok(None);
        }
        if params.command == symbol_documentation::CMD_GET_DOCUMENTATION_URI {
            let args = params.arguments;
            let uri = symbol_documentation::uri_from_command_args(&Some(args.clone()))?;
            let state = self.state.read().await;
            let doc = state.docs.get(&uri);
            return symbol_documentation::handle_symbol_documentation_command(&params.command, Some(args), doc, &uri);
        }
        let roots = self.workspace_roots.read().await.clone();
        if let Some(result) = pckg_registry::handle_pckg_registry_command(
            &params.command,
            Some(params.arguments.clone()),
            &roots,
            &self.pckg_registry,
        )
        .await?
        {
            return Ok(Some(result));
        }
        let explorer_result = with_shared_project_db(&self.state, None, true, |db| {
            handle_project_explorer_command(&params.command, Some(params.arguments), &roots, Some(db))
        })
        .await?;
        Ok(explorer_result)
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Some(level) = params
            .settings
            .get("beskid")
            .and_then(|v| v.get("lsp"))
            .and_then(|v| v.get("log"))
            .and_then(|v| v.get("level"))
            .and_then(|v| v.as_str())
        {
            *self.log_filter.write().await = ClientLogFilter::parse(level);
        }
        if let Some(focused) = focused_project_from_configuration(&params.settings) {
            let changed = {
                let mut state = self.state.write().await;
                let changed = state.focused_project != focused;
                state.focused_project = focused;
                changed
            };
            if changed {
                crate::session::project_context::invalidate_compilation_cache(&self.state).await;
                crate::session::lifecycle::rebuild_open_document_syntax_facts(&self.state).await;
            }
        }
    }

    // The scan runs again only when the set of roots changed: an event that adds and removes
    // nothing (clients send them) must not cost a walk of the workspace.
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let DidChangeWorkspaceFoldersParams { event } = params;
        for removed in &event.removed {
            if let Some(path) = uri_to_path(&removed.uri) {
                clear_closed_workspace_under_root(&self.client, &self.state, &path).await;
            }
        }
        let mut roots = self.workspace_roots.write().await;
        let before = roots.clone();
        for added in event.added {
            if let Some(path) = uri_to_path(&added.uri)
                && !roots.contains(&path)
            {
                roots.push(path);
            }
        }
        for removed in event.removed {
            if let Some(path) = uri_to_path(&removed.uri) {
                roots.retain(|item| item != &path);
            }
        }
        let changed = *roots != before;
        drop(roots);
        if changed {
            self.refresh_workspace_scan().await;
        }
    }

    // A changed manifest or lock is answered by one workspace scan, which types each project
    // once for all its closed files and republishes the open documents; it never prepares the
    // whole workspace file by file.
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let changed: Vec<PathBuf> = params.changes.into_iter().filter_map(|change| uri_to_path(&change.uri)).collect();
        if refresh_after_disk_change(&self.client, &self.state, &changed).await == DiskRefresh::ScanWorkspace {
            self.refresh_workspace_scan().await;
        }
    }

    // Formatting is syntax-only: it reads the open text, never the analysis state, so it does
    // not wait for workspace discovery or analysis.
    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(text) = formatting::open_texts::open_text(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(self
            .formatting_result(&params.text_document.uri, formatting::handler::handle_document_formatting(&text))
            .await)
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(text) = formatting::open_texts::open_text(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(self
            .formatting_result(
                &params.text_document.uri,
                formatting::handler::handle_range_formatting(&text, params.range),
            )
            .await)
    }
}

impl Backend {
    /// The edits of a formatting request, or no edits with the refusal reason sent to the client.
    async fn formatting_result(
        &self,
        uri: &Uri,
        result: std::result::Result<Vec<TextEdit>, beskid_analysis::format::FormatError>,
    ) -> Option<Vec<TextEdit>> {
        match result {
            Ok(edits) => Some(edits),
            Err(error) => {
                let document =
                    uri_to_path(uri).map_or_else(|| uri.as_str().to_string(), |path| path.display().to_string());
                let (kind, message) = formatting::handler::refusal_message(&document, &error);
                let filter = *self.log_filter.read().await;
                client_log(&self.client, filter, kind, message.clone()).await;
                if kind == MessageType::WARNING {
                    self.client.show_message(kind, message).await;
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod scan_tests {
    use std::str::FromStr;

    use tower_lsp_server::LspService;

    use super::*;

    fn completion_params(uri: &str) -> CompletionParams {
        CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: Uri::from_str(uri).expect("uri") },
                position: Position { line: 0, character: 0 },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        }
    }

    #[tokio::test]
    async fn completion_answers_while_a_scan_is_in_progress() {
        let (service, _socket) = LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let backend = service.inner();
        backend.signal_workspace_ready_for_tests().await;
        // A scan is running and a background scan task is yielding to interactive work.
        assert!(backend.scan.request());
        let answer =
            tokio::time::timeout(Duration::from_secs(2), backend.completion(completion_params("file:///tmp/A.bd")))
                .await
                .expect("completion must not wait for the scan")
                .expect("completion result");
        assert!(answer.is_some());
        assert_eq!(backend.interactive.active(), 0, "guard released");
    }

    fn semantic_tokens_params(uri: &Uri) -> SemanticTokensParams {
        SemanticTokensParams {
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        }
    }

    fn open_text_for_tests(uri: &Uri, version: i32, text: &str) {
        observe_text_sync_for_tests(
            "textDocument/didOpen",
            &DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "beskid".into(),
                    version,
                    text: text.into(),
                },
            },
        );
    }

    fn close_text_for_tests(uri: &Uri) {
        observe_text_sync_for_tests(
            "textDocument/didClose",
            &DidCloseTextDocumentParams { text_document: TextDocumentIdentifier { uri: uri.clone() } },
        );
    }

    fn analyzed_main(text: &str, start: usize) -> crate::session::store::Document {
        crate::session::store::Document {
            version: 1,
            text: text.into(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: vec![crate::session::store::SyntaxSymbol {
                name: "main".into(),
                kind: beskid_analysis::services::AnalysisSymbolKind::Function,
                start,
                end: start + 4,
            }],
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        }
    }

    // A request during a burst of edits used to wait for the analysis of the newest text, and
    // every further edit moved that target (16 s in the preview.10 soak).
    #[tokio::test]
    async fn semantic_tokens_answer_from_carried_facts_while_the_newest_text_is_analyzed() {
        let (service, _socket) = LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-semtok-stale/Main.bd").expect("uri");
        backend.state.write().await.docs.insert(uri.clone(), analyzed_main("fn main() {}", 3));
        // The client sent a newer text; its analysis has not finished.
        open_text_for_tests(&uri, 2, "// edit\nfn main() {}");
        let started = std::time::Instant::now();
        let answer =
            tokio::time::timeout(Duration::from_secs(2), backend.semantic_tokens_full(semantic_tokens_params(&uri)))
                .await
                .expect("semantic tokens must not wait for the analysis of the newest text")
                .expect("semantic tokens result");
        close_text_for_tests(&uri);
        assert!(started.elapsed() < Duration::from_secs(1), "answered within the facts budget");
        let Some(SemanticTokensResult::Tokens(tokens)) = answer else { panic!("expected full tokens") };
        assert_eq!(tokens.data.len(), 1, "the carried declaration token");
        assert_eq!((tokens.data[0].delta_line, tokens.data[0].delta_start, tokens.data[0].length), (1, 3, 4));
    }

    #[tokio::test]
    async fn a_stale_token_answer_asks_a_refreshing_client_to_refresh_once_the_facts_are_current() {
        use futures_util::StreamExt;
        use tower_service::Service;

        let (mut service, mut socket) =
            LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let (refreshes, mut refreshed) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(request) = socket.next().await {
                if request.method() == "workspace/semanticTokens/refresh" {
                    let _ = refreshes.send(());
                }
            }
        });
        let init = tower_lsp_server::jsonrpc::Request::build("initialize")
            .params(serde_json::json!({
                "processId": null,
                "capabilities": {"workspace": {"semanticTokens": {"refreshSupport": true}}}
            }))
            .id(1)
            .finish();
        let response = service.call(init).await.expect("initialize service").expect("initialize response");
        assert!(response.error().is_none(), "{response:?}");
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-semtok-refresh/Main.bd").expect("uri");
        backend.state.write().await.docs.insert(uri.clone(), analyzed_main("fn main() {}", 3));
        let newest = "// edit\nfn main() {}";
        open_text_for_tests(&uri, 2, newest);
        let stale = backend.semantic_tokens_full(semantic_tokens_params(&uri)).await.expect("result");
        assert!(stale.is_some(), "the carried tokens answer");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(refreshed.try_recv().is_err(), "no refresh before the newest facts are stored");
        // The analysis of the newest text stores its facts.
        backend.state.write().await.docs.insert(uri.clone(), analyzed_main(newest, 11));
        backend.state.read().await.facts_stored.notify_waiters();
        let refresh = tokio::time::timeout(Duration::from_secs(2), refreshed.recv()).await;
        close_text_for_tests(&uri);
        assert!(matches!(refresh, Ok(Some(()))), "one refresh once the facts are current");
    }

    #[tokio::test]
    async fn semantic_tokens_of_a_buffer_without_facts_answer_null_at_once() {
        let (service, _socket) = LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-semtok-unanalyzed/Main.bd").expect("uri");
        open_text_for_tests(&uri, 1, "fn main() {}");
        let answer =
            tokio::time::timeout(Duration::from_secs(2), backend.semantic_tokens_full(semantic_tokens_params(&uri)))
                .await
                .expect("semantic tokens must not wait for the first analysis")
                .expect("semantic tokens result");
        close_text_for_tests(&uri);
        assert!(answer.is_none());
    }

    #[tokio::test]
    async fn semantic_tokens_of_current_facts_answer_from_them() {
        let (service, _socket) = LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-semtok-current/Main.bd").expect("uri");
        backend.state.write().await.docs.insert(uri.clone(), analyzed_main("fn main() {}", 3));
        open_text_for_tests(&uri, 1, "fn main() {}");
        let answer = backend.semantic_tokens_full(semantic_tokens_params(&uri)).await.expect("result");
        close_text_for_tests(&uri);
        let Some(SemanticTokensResult::Tokens(tokens)) = answer else { panic!("expected full tokens") };
        assert_eq!((tokens.data[0].delta_line, tokens.data[0].delta_start, tokens.data[0].length), (0, 3, 4));
    }

    // ---- Requests during an edit burst -------------------------------------------------------
    //
    // `BURST_BASE` is the analyzed text; each edit of a burst puts a new `// edit N` line in front
    // of it. The analysis of every edit finishes one edit late, so the stored facts never match
    // the newest text while the burst runs: the request that used to wait for "the facts of the
    // newest text" (up to 120 s) chased the burst to its end.

    //                         0         1         2
    //                         0123456789012 3456789012345678901
    const BURST_BASE: &str = "fn main() {}\nfn f() { main(); }\n";
    const BURST_EDITS: i32 = 40;
    const BURST_EVERY: Duration = Duration::from_millis(50);

    fn burst_prefix(version: i32) -> String {
        if version <= 1 { String::new() } else { format!("// edit {version}\n") }
    }

    fn burst_text(version: i32) -> String {
        format!("{}{BURST_BASE}", burst_prefix(version))
    }

    /// The facts of `burst_text(version)`: `main` declared at 3..7 and called at 22..26.
    fn burst_facts(uri: &Uri, version: i32) -> crate::session::store::Document {
        let shift = burst_prefix(version).len();
        let path = uri_to_path(uri).expect("file uri");
        let mut document = analyzed_main(&burst_text(version), 3 + shift);
        document.version = version;
        document.syntax_definitions = vec![crate::session::store::SyntaxDefinition {
            reference_start: 22 + shift,
            reference_end: 26 + shift,
            declaration_path: path.clone(),
            declaration_start: 3 + shift,
            declaration_end: 7 + shift,
        }];
        document.syntax_hovers = vec![crate::session::store::SyntaxHover {
            reference_start: 22 + shift,
            reference_end: 26 + shift,
            markdown: "fn main()".into(),
            location_path: path,
            location_start: 3 + shift,
            location_end: 7 + shift,
        }];
        document.syntax_inlay_hints = vec![crate::session::store::SyntaxInlayHint {
            start: 3 + shift,
            end: 7 + shift,
            type_label: "unit".into(),
        }];
        document
    }

    fn change_text_for_tests(uri: &Uri, version: i32, text: &str) {
        observe_text_sync_for_tests(
            "textDocument/didChange",
            &DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri: uri.clone(), version },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.into(),
                }],
            },
        );
    }

    async fn store_facts_for_tests(backend: &Backend, uri: &Uri, document: crate::session::store::Document) {
        let mut state = backend.state.write().await;
        state.docs.insert(uri.clone(), document);
        state.facts_stored.notify_waiters();
    }

    /// Open `uri` with analyzed facts for version 1, then run `request` while a burst of edits
    /// runs; each stored analysis is one edit behind the newest text. Returns the answer and
    /// how long the request took.
    async fn during_an_edit_burst<T>(
        backend: &Backend,
        uri: &Uri,
        request: impl std::future::Future<Output = T>,
    ) -> (T, Duration) {
        store_facts_for_tests(backend, uri, burst_facts(uri, 1)).await;
        open_text_for_tests(uri, 1, &burst_text(1));
        change_text_for_tests(uri, 2, &burst_text(2));
        let burst = async {
            for version in 3..=BURST_EDITS + 1 {
                tokio::time::sleep(BURST_EVERY).await;
                change_text_for_tests(uri, version, &burst_text(version));
                store_facts_for_tests(backend, uri, burst_facts(uri, version - 1)).await;
            }
        };
        let timed = async {
            // Let the burst start before the request arrives.
            tokio::time::sleep(BURST_EVERY / 2).await;
            let started = std::time::Instant::now();
            let answer = request.await;
            (answer, started.elapsed())
        };
        let ((), (answer, elapsed)) = tokio::join!(burst, timed);
        close_text_for_tests(uri);
        assert!(
            elapsed < Duration::from_secs(1),
            "answered during the burst in {elapsed:?}, the burst lasts {:?}",
            BURST_EVERY * (BURST_EDITS as u32)
        );
        (answer, elapsed)
    }

    fn position_params(uri: &Uri, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        }
    }

    fn assert_content_modified<T: std::fmt::Debug>(answer: Result<T>) {
        match answer {
            Err(error) => assert_eq!(error.code.code(), -32801, "ContentModified, got {error:?}"),
            Ok(answer) => panic!("an edit-producing request answered from facts of an older text: {answer:?}"),
        }
    }

    fn backend_for_tests() -> (LspService<Backend>, tower_lsp_server::ClientSocket) {
        LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))))
    }

    #[tokio::test]
    async fn references_answer_from_carried_facts_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-references/Main.bd").expect("uri");
        let params = ReferenceParams {
            text_document_position: position_params(&uri, 2, 10),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: ReferenceContext { include_declaration: true },
        };
        let (answer, _) = during_an_edit_burst(backend, &uri, backend.references(params)).await;
        let locations = answer.expect("references result").expect("locations");
        let mut lines: Vec<_> = locations.iter().map(|location| location.range.start).collect();
        lines.sort_by_key(|position| (position.line, position.character));
        assert_eq!(
            lines,
            vec![Position { line: 1, character: 3 }, Position { line: 2, character: 9 }],
            "the declaration and the call, carried below the inserted line"
        );
    }

    #[tokio::test]
    async fn signature_help_answers_from_carried_facts_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-signature/Main.bd").expect("uri");
        let params = SignatureHelpParams {
            context: None,
            text_document_position_params: position_params(&uri, 2, 14),
            work_done_progress_params: Default::default(),
        };
        let (answer, _) = during_an_edit_burst(backend, &uri, backend.signature_help(params)).await;
        let help = answer.expect("signature help result").expect("signature");
        assert_eq!(help.signatures[0].label, "fn main()");
    }

    #[tokio::test]
    async fn inlay_hints_answer_from_carried_facts_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-inlay/Main.bd").expect("uri");
        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 9, character: 0 } },
        };
        let (answer, _) = during_an_edit_burst(backend, &uri, backend.inlay_hint(params)).await;
        let hints = answer.expect("inlay hint result").expect("hints");
        assert_eq!(hints.len(), 1, "the carried hint");
        assert_eq!(hints[0].position, Position { line: 1, character: 7 }, "moved below the inserted line");
    }

    #[tokio::test]
    async fn semantic_tokens_answer_from_carried_facts_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-semtok/Main.bd").expect("uri");
        let (answer, _) =
            during_an_edit_burst(backend, &uri, backend.semantic_tokens_full(semantic_tokens_params(&uri))).await;
        let Some(SemanticTokensResult::Tokens(tokens)) = answer.expect("semantic tokens result") else {
            panic!("expected full tokens")
        };
        assert_eq!((tokens.data[0].delta_line, tokens.data[0].delta_start), (1, 3));
    }

    #[tokio::test]
    async fn rename_fails_content_modified_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-rename/Main.bd").expect("uri");
        let params = RenameParams {
            text_document_position: position_params(&uri, 2, 10),
            new_name: "start".into(),
            work_done_progress_params: Default::default(),
        };
        let (answer, _) = during_an_edit_burst(backend, &uri, backend.rename(params)).await;
        assert_content_modified(answer);
    }

    #[tokio::test]
    async fn prepare_rename_fails_content_modified_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-prepare/Main.bd").expect("uri");
        let (answer, _) =
            during_an_edit_burst(backend, &uri, backend.prepare_rename(position_params(&uri, 2, 10))).await;
        assert_content_modified(answer);
    }

    #[tokio::test]
    async fn code_actions_fail_content_modified_during_an_edit_burst() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-actions/Main.bd").expect("uri");
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position { line: 1, character: 3 }, end: Position { line: 1, character: 3 } },
            context: CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let (answer, _) = during_an_edit_burst(backend, &uri, backend.code_action(params)).await;
        assert_content_modified(answer);
    }

    #[tokio::test]
    async fn rename_of_current_facts_edits_the_newest_text() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-rename-current/Main.bd").expect("uri");
        store_facts_for_tests(backend, &uri, burst_facts(&uri, 1)).await;
        open_text_for_tests(&uri, 1, &burst_text(1));
        change_text_for_tests(&uri, 2, &burst_text(2));
        // The analysis of version 2 finishes within the budget of the rename.
        let analysis = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            store_facts_for_tests(backend, &uri, burst_facts(&uri, 2)).await;
        };
        let params = RenameParams {
            text_document_position: position_params(&uri, 2, 10),
            new_name: "start".into(),
            work_done_progress_params: Default::default(),
        };
        let ((), answer) = tokio::join!(analysis, backend.rename(params));
        close_text_for_tests(&uri);
        let edit = answer.expect("rename result").expect("workspace edit");
        let mut starts: Vec<_> = edit.changes.expect("changes")[&uri].iter().map(|edit| edit.range.start).collect();
        starts.sort_by_key(|position| (position.line, position.character));
        assert_eq!(starts, vec![Position { line: 1, character: 3 }, Position { line: 2, character: 9 }]);
    }

    #[tokio::test]
    async fn edits_computed_before_a_client_edit_are_never_returned() {
        let (service, _socket) = backend_for_tests();
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-rename-race/Main.bd").expect("uri");
        store_facts_for_tests(backend, &uri, burst_facts(&uri, 1)).await;
        open_text_for_tests(&uri, 1, &burst_text(1));
        let current = backend.edit_facts(&uri).await.expect("current facts").expect("document");
        assert!(unchanged_since(&uri, &current).is_ok());
        // The client edits while the rename computes its edits from `current`.
        change_text_for_tests(&uri, 2, &burst_text(2));
        assert_content_modified(unchanged_since(&uri, &current));
        close_text_for_tests(&uri);
    }

    #[tokio::test]
    async fn a_stale_inlay_hint_answer_asks_a_refreshing_client_to_refresh() {
        use futures_util::StreamExt;
        use tower_service::Service;

        let (mut service, mut socket) = backend_for_tests();
        let (refreshes, mut refreshed) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(request) = socket.next().await {
                if request.method() == "workspace/inlayHint/refresh" {
                    let _ = refreshes.send(());
                }
            }
        });
        let init = tower_lsp_server::jsonrpc::Request::build("initialize")
            .params(serde_json::json!({
                "processId": null,
                "capabilities": {"workspace": {"inlayHint": {"refreshSupport": true}}}
            }))
            .id(1)
            .finish();
        let response = service.call(init).await.expect("initialize service").expect("initialize response");
        assert!(response.error().is_none(), "{response:?}");
        let backend = service.inner();
        let uri = Uri::from_str("file:///tmp/beskid-lspwait-inlay-refresh/Main.bd").expect("uri");
        store_facts_for_tests(backend, &uri, burst_facts(&uri, 1)).await;
        open_text_for_tests(&uri, 1, &burst_text(1));
        change_text_for_tests(&uri, 2, &burst_text(2));
        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 9, character: 0 } },
        };
        let stale = backend.inlay_hint(params).await.expect("result").expect("hints");
        assert_eq!(stale.len(), 1, "the carried hint answers");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(refreshed.try_recv().is_err(), "no refresh before the newest facts are stored");
        store_facts_for_tests(backend, &uri, burst_facts(&uri, 2)).await;
        let refresh = tokio::time::timeout(Duration::from_secs(2), refreshed.recv()).await;
        close_text_for_tests(&uri);
        assert!(matches!(refresh, Ok(Some(()))), "one refresh once the facts are current");
    }

    #[tokio::test]
    async fn scan_triggers_during_a_scan_do_not_wait_and_coalesce() {
        let (service, _socket) = LspService::new(|client| Backend::new(client, Arc::new(CrashReporter::new(None))));
        let backend = service.inner();
        assert!(backend.scan.request(), "a scan is running");
        for _ in 0..3 {
            tokio::time::timeout(Duration::from_secs(1), backend.refresh_workspace_scan())
                .await
                .expect("a trigger during a scan returns at once");
        }
        assert!(backend.scan.finish_round(), "three triggers leave exactly one follow-up round");
        assert!(!backend.scan.finish_round());
    }
}
