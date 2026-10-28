//! The text of every open document, kept apart from the analysis state.
//!
//! Formatting and the document outline are syntax-only, so they must not wait for workspace
//! discovery, the analysis state lock or a slow `didOpen`. The transport service feeds every text-sync notification here
//! synchronously, in the order the client sent it (see `TextSyncOrderedService::call`), and the
//! formatting and outline handlers read the text from here.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::de::DeserializeOwned;
use tower_lsp_server::jsonrpc::Request;
use tower_lsp_server::ls_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Uri,
};

use crate::text_sync::apply_document_changes;

/// One language server runs per process, so the open texts are process-wide.
static OPEN_TEXTS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Requests answered from the open text alone; they do not wait for earlier text-sync work.
pub(crate) fn is_syntax_only_request(method: &str) -> bool {
    matches!(method, "textDocument/formatting" | "textDocument/rangeFormatting" | "textDocument/documentSymbol")
}

fn params<T: DeserializeOwned>(request: &Request) -> Option<T> {
    request.params().and_then(|value| serde_json::from_value(value.clone()).ok())
}

/// Apply a `didOpen`, `didChange` or `didClose` notification to the open texts.
pub(crate) fn observe(request: &Request) {
    match request.method() {
        "textDocument/didOpen" => {
            if let Some(p) = params::<DidOpenTextDocumentParams>(request) {
                texts().insert(p.text_document.uri.as_str().to_string(), p.text_document.text);
            }
        }
        "textDocument/didChange" => {
            if let Some(p) = params::<DidChangeTextDocumentParams>(request) {
                let mut texts = texts();
                let key = p.text_document.uri.as_str().to_string();
                match texts.get_mut(&key) {
                    Some(text) => apply_document_changes(text, p.content_changes),
                    None => {
                        // Without an open text only a full replacement can be applied.
                        if let Some(full) = p.content_changes.into_iter().rev().find(|c| c.range.is_none()) {
                            texts.insert(key, full.text);
                        }
                    }
                }
            }
        }
        "textDocument/didClose" => {
            if let Some(p) = params::<DidCloseTextDocumentParams>(request) {
                texts().remove(p.text_document.uri.as_str());
            }
        }
        _ => {}
    }
}

/// The current text of an open document.
pub(crate) fn open_text(uri: &Uri) -> Option<String> {
    texts().get(uri.as_str()).cloned()
}

fn texts() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    // A panic while holding the lock cannot leave a half-applied edit worse than a stale text;
    // keep serving rather than poisoning formatting for the rest of the session.
    OPEN_TEXTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn notification(method: &'static str, params: serde_json::Value) -> Request {
        Request::build(method).params(params).finish()
    }

    #[test]
    fn open_change_close_track_the_client_text_in_order() {
        let uri = "file:///tmp/open-texts-test/Main.bd";
        observe(&notification(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "beskid", "version": 1, "text": "i32 A() { return 1; }\n"}}),
        ));
        observe(&notification(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri, "version": 2},
                "contentChanges": [{"range": {"start": {"line": 0, "character": 17}, "end": {"line": 0, "character": 18}}, "text": "2"}]
            }),
        ));
        let parsed: Uri = uri.parse().expect("uri");
        assert_eq!(open_text(&parsed).as_deref(), Some("i32 A() { return 2; }\n"));
        observe(&notification("textDocument/didClose", json!({"textDocument": {"uri": uri}})));
        assert_eq!(open_text(&parsed), None);
    }

    #[test]
    fn only_syntax_only_requests_skip_the_text_sync_fence() {
        assert!(is_syntax_only_request("textDocument/formatting"));
        assert!(is_syntax_only_request("textDocument/rangeFormatting"));
        assert!(is_syntax_only_request("textDocument/documentSymbol"));
        assert!(!is_syntax_only_request("textDocument/hover"));
        assert!(!is_syntax_only_request("textDocument/didChange"));
    }
}
