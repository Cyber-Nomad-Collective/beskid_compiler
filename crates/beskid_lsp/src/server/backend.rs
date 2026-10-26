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
use crate::protocol::request::{
    current_document, latest_document, snapshot_document, snapshot_lsp_request, snapshot_request,
};
use crate::scan_schedule::{InteractiveGuard, ScanCoordinator, ScanRunGuard};
use crate::server::init::initialize_result;
use crate::session::db_access::{parallel_project_db, with_project_db};
use crate::session::lifecycle::{analyze_open_document, publish_diagnostics_for_uri, remove_document};
use crate::session::mod_host::mod_host_from_initialization_options;
use crate::session::store::State;
use crate::workspace_scan::{
    clear_closed_workspace_under_root, hydrate_disk_after_close, refresh_after_disk_change, scan_workspace_roots,
    uri_to_path,
};

/// Longest completion waits for the analysis of the newest text before it answers from the
/// last facts carried over the edit.
const COMPLETION_FACTS_WAIT: Duration = Duration::from_millis(100);
/// Longest completion waits for its project's database, busy with another analysis.
const COMPLETION_DB_WAIT: Duration = Duration::from_millis(100);

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
        }
    }

    /// Unblock analysis paths that wait on the initial workspace scan (integration tests only).
    pub async fn signal_workspace_ready_for_tests(&self) {
        crate::session::startup::signal_initial_scan_complete(&self.state).await;
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

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
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

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(definition::handler::handle_definition(&snapshot.uri, &snapshot.document, snapshot.offset))
    }

    async fn goto_declaration(
        &self,
        params: request::GotoDeclarationParams,
    ) -> Result<Option<request::GotoDeclarationResponse>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(definition::handler::handle_definition(&snapshot.uri, &snapshot.document, snapshot.offset))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let include_declaration = params.context.include_declaration;
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(Some(Vec::new()));
        };
        let entry_path = uri_to_path(&snapshot.uri);
        Ok(Some(references::handler::handle_references(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
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
        let (document, current) = match current_document(&self.state, &uri, COMPLETION_FACTS_WAIT).await {
            Ok(Some(document)) => (document, true),
            Ok(None) => return Ok(Some(CompletionResponse::Array(Vec::new()))),
            Err(carried) => (carried, false),
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
        } else {
            None
        };
        let response = tokio::task::spawn_blocking(move || {
            crate::crash::run_blocking_recorded(|| {
                salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    completion::handler::handle_completion(db.as_ref(), &uri, &document, offset)
                }))
            })
        })
        .await
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
    // not wait for the analysis of an earlier edit.
    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let text = match formatting::open_texts::open_text(&uri) {
            Some(text) => text,
            None => match snapshot_document(&self.state, &uri).await {
                Some(document) => document.text,
                None => return Ok(Some(DocumentSymbolResponse::Nested(Vec::new()))),
            },
        };
        Ok(Some(document_symbols::handler::document_symbols_for_text(&uri, &text)))
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(None);
        };
        Ok(Some(semantic_tokens::handler::handle_semantic_tokens(&uri, &document)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.clone();
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(inlay_hints::handler::handle_inlay_hints(&uri, &document, &params)))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(signature_help::handler::handle_signature_help(&snapshot.uri, &snapshot.document, snapshot.offset))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(code_actions::handler::handle_code_actions(&uri, &document, &params)))
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        let state = self.state.read().await;
        Ok(Some(workspace_symbols::handler::handle_workspace_symbols(&state, params)))
    }

    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> Result<Option<PrepareRenameResponse>> {
        let Some(snapshot) = snapshot_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(rename::handler::handle_prepare_rename(&snapshot.uri, &snapshot.document, snapshot.offset))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let new_name = params.new_name.clone();
        let request = params.text_document_position;
        let Some(snapshot) = snapshot_lsp_request(&self.state, request.clone()).await else {
            return Ok(None);
        };
        Ok(rename::handler::handle_rename(&snapshot.uri, &snapshot.document, request.position, &new_name))
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
        let explorer_result = with_project_db(&self.state, None, true, |db| {
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

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let DidChangeWorkspaceFoldersParams { event } = params;
        for removed in &event.removed {
            if let Some(path) = uri_to_path(&removed.uri) {
                clear_closed_workspace_under_root(&self.client, &self.state, &path).await;
            }
        }
        let mut roots = self.workspace_roots.write().await;
        for added in event.added {
            if let Some(path) = uri_to_path(&added.uri) {
                roots.push(path);
            }
        }
        for removed in event.removed {
            if let Some(path) = uri_to_path(&removed.uri) {
                roots.retain(|item| item != &path);
            }
        }
        drop(roots);
        self.refresh_workspace_scan().await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let changed: Vec<PathBuf> = params.changes.into_iter().filter_map(|change| uri_to_path(&change.uri)).collect();
        refresh_after_disk_change(&self.client, &self.state, &changed).await;
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
