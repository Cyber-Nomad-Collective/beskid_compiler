use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::features::{
    code_actions, completion, definition, document_symbols, formatting, hover, inlay_hints,
    references, semantic_tokens, signature_help, workspace_symbols,
};
use crate::logging::{ClientLogFilter, client_log};
use crate::protocol::request::{snapshot_document, snapshot_lsp_request};
use crate::server::init::initialize_result;
use crate::session::lifecycle::{publish_diagnostics_for_uri, remove_document, set_document};
use crate::session::store::State;
use crate::text_sync::apply_document_changes;
use crate::workspace_scan::{
    clear_closed_workspace_under_root, hydrate_disk_after_close, refresh_after_disk_change,
    scan_workspace, uri_to_path,
};

pub struct Backend {
    client: Client,
    state: Arc<RwLock<State>>,
    workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    log_filter: Arc<RwLock<ClientLogFilter>>,
    diagnostics_revision: Arc<Mutex<HashMap<Uri, u64>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(State::default())),
            workspace_roots: Arc::new(RwLock::new(Vec::new())),
            log_filter: Arc::new(RwLock::new(ClientLogFilter::Info)),
            diagnostics_revision: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn schedule_publish_diagnostics(&self, uri: Uri) {
        let rev = {
            let mut map = self.diagnostics_revision.lock().await;
            let next = map.get(&uri).copied().unwrap_or(0).saturating_add(1);
            map.insert(uri.clone(), next);
            next
        };

        let client = self.client.clone();
        let state = self.state.clone();
        let revisions = self.diagnostics_revision.clone();
        let filter = *self.log_filter.read().await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            let should_run = {
                let map = revisions.lock().await;
                map.get(&uri).copied() == Some(rev)
            };
            if should_run {
                client_log(
                    &client,
                    filter,
                    MessageType::LOG,
                    format!("publishing diagnostics for {}", uri.as_str()),
                )
                .await;
                publish_diagnostics_for_uri(&client, &state, &uri).await;
            }
        });
    }

    async fn refresh_workspace_scan(&self) {
        let roots = { self.workspace_roots.read().await.clone() };
        let filter = *self.log_filter.read().await;
        for root in roots {
            client_log(
                &self.client,
                filter,
                MessageType::INFO,
                format!("workspace scan started: {}", root.display()),
            )
            .await;
            scan_workspace(&self.client, &self.state, &root).await;
            client_log(
                &self.client,
                filter,
                MessageType::INFO,
                format!("workspace scan completed: {}", root.display()),
            )
            .await;
        }
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

        if let Some(options) = params.initialization_options
            && let Some(level) = options.get("logLevel").and_then(|v| v.as_str())
        {
            *self.log_filter.write().await = ClientLogFilter::parse(level);
        }
        Ok(initialize_result())
    }

    async fn initialized(&self, _: InitializedParams) {
        let filter = *self.log_filter.read().await;
        client_log(&self.client, filter, MessageType::INFO, "Beskid LSP initialized".to_string())
            .await;
        self.refresh_workspace_scan().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        set_document(&self.state, doc.uri.clone(), doc.version, doc.text).await;
        self.schedule_publish_diagnostics(doc.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content_changes = params.content_changes;
        if let Some(mut doc) = snapshot_document(&self.state, &uri).await {
            apply_document_changes(&mut doc.text, content_changes);
            set_document(
                &self.state,
                uri.clone(),
                params.text_document.version,
                doc.text,
            )
            .await;
            self.schedule_publish_diagnostics(uri).await;
        } else if let Some(full_text) = content_changes
            .into_iter()
            .rev()
            .find(|change| change.range.is_none())
            .map(|change| change.text)
        {
            set_document(&self.state, uri.clone(), params.text_document.version, full_text).await;
            self.schedule_publish_diagnostics(uri).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.schedule_publish_diagnostics(params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        remove_document(&self.state, &uri).await;
        hydrate_disk_after_close(&self.client, &self.state, &uri).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(hover::handler::handle_hover(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
        ))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(definition::handler::handle_definition(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
        ))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let include_declaration = params.context.include_declaration;
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(references::handler::handle_references(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
            include_declaration,
        )))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        };
        Ok(Some(completion::handler::handle_completion(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
        )))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(Some(DocumentSymbolResponse::Nested(Vec::new())));
        };
        Ok(Some(document_symbols::handler::handle_document_symbols(
            &uri, &document,
        )))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(None);
        };
        Ok(Some(semantic_tokens::handler::handle_semantic_tokens(
            &document,
        )))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.clone();
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(inlay_hints::handler::handle_inlay_hints(
            &uri, &document, &params,
        )))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let Some(snapshot) = snapshot_lsp_request(&self.state, params).await else {
            return Ok(None);
        };
        Ok(signature_help::handler::handle_signature_help(
            &snapshot.uri,
            &snapshot.document,
            snapshot.offset,
        ))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(code_actions::handler::handle_code_actions(
            &uri, &document, &params,
        )))
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        let state = self.state.read().await;
        Ok(Some(workspace_symbols::handler::handle_workspace_symbols(
            &state, params,
        )))
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<LSPAny>> {
        if params.command == "beskid.refreshWorkspace" {
            self.refresh_workspace_scan().await;
            return Ok(None);
        }
        Ok(None)
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
        let changed: Vec<PathBuf> = params
            .changes
            .into_iter()
            .filter_map(|change| uri_to_path(&change.uri))
            .collect();
        refresh_after_disk_change(&self.client, &self.state, &changed).await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(None);
        };
        Ok(formatting::handler::handle_document_formatting(&document))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(document) = snapshot_document(&self.state, &uri).await else {
            return Ok(None);
        };
        Ok(formatting::handler::handle_range_formatting(
            &document,
            params.range,
        ))
    }
}
