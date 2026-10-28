use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::{TextDocumentPositionParams, Uri};

use crate::features::formatting::open_texts::open_text;
use crate::position::position_to_offset;
use crate::protocol::params::IntoTextDocumentPosition;
use crate::session::store::{Document, State};
use crate::workspace_scan::uri_to_path;

#[derive(Debug, Clone)]
pub struct DocumentRequestSnapshot {
    pub uri: Uri,
    pub document: Document,
    pub offset: usize,
}

/// Longest a request waits for the analysis of an open document's newest text.
const CURRENT_FACTS_WAIT: Duration = Duration::from_secs(120);

/// The document with the facts of its newest text. An open document is analyzed in the
/// background after each edit; this waits until the facts of the text the client last sent are
/// stored (`None` when that takes longer than [`CURRENT_FACTS_WAIT`]).
pub async fn snapshot_document(state: &RwLock<State>, uri: &Uri) -> Option<Document> {
    current_document(state, uri, CURRENT_FACTS_WAIT).await.ok()?
}

/// The document with the facts of its newest text if they arrive within `wait`; otherwise
/// `Err` with the last analyzed facts carried over to the newest text.
///
/// `Ok(None)` when the buffer has no facts at all within `wait`.
pub async fn current_document(state: &RwLock<State>, uri: &Uri, wait: Duration) -> Result<Option<Document>, Document> {
    let started = tokio::time::Instant::now();
    loop {
        let stored = Arc::clone(&state.read().await.facts_stored);
        let notified = stored.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let document = state.read().await.document_union(uri);
        let newest = open_text(uri);
        match (document, newest) {
            (document, None) => return Ok(document),
            (Some(document), Some(newest)) if document.text == newest => return Ok(Some(document)),
            (document, Some(newest)) => {
                let deadline = started + wait;
                if tokio::time::timeout_at(deadline, notified).await.is_err() {
                    return match document {
                        Some(document) => Err(document.carried_over(newest, uri_to_path(uri).as_deref())),
                        None => Ok(None),
                    };
                }
            }
        }
    }
}

/// The last analyzed facts, carried over to the newest text when its analysis is still
/// running. Never waits for analysis.
pub async fn latest_document(state: &RwLock<State>, uri: &Uri) -> Option<Document> {
    let document = state.read().await.document_union(uri)?;
    match open_text(uri) {
        Some(newest) if newest != document.text => Some(document.carried_over(newest, uri_to_path(uri).as_deref())),
        _ => Some(document),
    }
}

pub async fn snapshot_request(
    state: &RwLock<State>,
    request: TextDocumentPositionParams,
) -> Option<DocumentRequestSnapshot> {
    let uri = request.text_document.uri;
    let document = snapshot_document(state, &uri).await?;
    let offset = position_to_offset(&document.text, request.position);
    Some(DocumentRequestSnapshot { uri, document, offset })
}

pub async fn snapshot_lsp_request<P>(state: &RwLock<State>, request: P) -> Option<DocumentRequestSnapshot>
where
    P: IntoTextDocumentPosition,
{
    snapshot_request(state, request.into_text_document_position()).await
}
