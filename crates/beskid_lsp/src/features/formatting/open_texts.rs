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

/// The newest text of an open document and the version the client gave it.
struct OpenText {
    version: i32,
    text: String,
}

/// One language server runs per process, so the open texts are process-wide.
static OPEN_TEXTS: LazyLock<Mutex<HashMap<String, OpenText>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Requests answered from the open text alone; they do not wait for earlier text-sync work.
///
/// Code actions are computed from the newest open text (current here, in transport order) or,
/// for a Mod quick fix, wait for the stored facts of that text on their own (bounded); neither
/// needs the earlier `didChange` handlers to have run. Behind those handlers they waited for
/// the runtime to schedule each of them in turn (up to 250 ms under load).
pub(crate) fn is_syntax_only_request(method: &str) -> bool {
    matches!(
        method,
        "textDocument/formatting"
            | "textDocument/rangeFormatting"
            | "textDocument/documentSymbol"
            | "textDocument/codeAction"
    )
}

fn params<T: DeserializeOwned>(request: &Request) -> Option<T> {
    request.params().and_then(|value| serde_json::from_value(value.clone()).ok())
}

/// How many text-sync notifications changed the open texts so far (any document).
static EDITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The number of open-text changes so far, of every document: two equal counts mean no open
/// text changed in between.
pub(crate) fn edit_count() -> u64 {
    EDITS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Apply a `didOpen`, `didChange` or `didClose` notification to the open texts.
pub(crate) fn observe(request: &Request) {
    if matches!(request.method(), "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose") {
        EDITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    match request.method() {
        "textDocument/didOpen" => {
            if let Some(p) = params::<DidOpenTextDocumentParams>(request) {
                let open = OpenText { version: p.text_document.version, text: p.text_document.text };
                texts().insert(p.text_document.uri.as_str().to_string(), open);
            }
        }
        "textDocument/didChange" => {
            if let Some(p) = params::<DidChangeTextDocumentParams>(request) {
                let mut texts = texts();
                let key = p.text_document.uri.as_str().to_string();
                let version = p.text_document.version;
                match texts.get_mut(&key) {
                    Some(open) => {
                        apply_document_changes(&mut open.text, p.content_changes);
                        open.version = version;
                    }
                    None => {
                        // Without an open text only a full replacement can be applied.
                        if let Some(full) = p.content_changes.into_iter().rev().find(|c| c.range.is_none()) {
                            texts.insert(key, OpenText { version, text: full.text });
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
    texts().get(uri.as_str()).map(|open| open.text.clone())
}

/// The current text of an open document and its version, read together.
pub(crate) fn open_text_and_version(uri: &Uri) -> Option<(String, i32)> {
    texts().get(uri.as_str()).map(|open| (open.text.clone(), open.version))
}

/// The version of the current text of an open document. It changes with every edit the
/// client sends, so a request that saw one version can tell that the text changed under it.
pub(crate) fn open_version(uri: &Uri) -> Option<i32> {
    texts().get(uri.as_str()).map(|open| open.version)
}

/// How `text` relates to the current text of the document, compared without copying either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenMatch {
    /// The document is not open: its disk text is the current text.
    Closed,
    /// `text` is the current open text, at this version.
    Current(i32),
    /// The client sent a newer text.
    Behind,
}

pub(crate) fn match_open_text(uri: &Uri, text: Option<&str>) -> OpenMatch {
    match texts().get(uri.as_str()) {
        None => OpenMatch::Closed,
        Some(open) if text == Some(open.text.as_str()) => OpenMatch::Current(open.version),
        Some(_) => OpenMatch::Behind,
    }
}

fn texts() -> std::sync::MutexGuard<'static, HashMap<String, OpenText>> {
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
        assert_eq!(open_version(&parsed), Some(2));
        assert_eq!(match_open_text(&parsed, Some("i32 A() { return 2; }\n")), OpenMatch::Current(2));
        assert_eq!(match_open_text(&parsed, Some("i32 A() { return 1; }\n")), OpenMatch::Behind);
        assert_eq!(match_open_text(&parsed, None), OpenMatch::Behind);
        observe(&notification("textDocument/didClose", json!({"textDocument": {"uri": uri}})));
        assert_eq!(open_text(&parsed), None);
        assert_eq!(open_version(&parsed), None);
        assert_eq!(match_open_text(&parsed, None), OpenMatch::Closed);
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
