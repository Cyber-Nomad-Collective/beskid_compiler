//! Semantic tokens answer within a short budget, from the last analyzed facts when the analysis
//! of the newest text is still running.
//!
//! LSP 3.16 lets a server answer `textDocument/semanticTokens/full` from facts that are not
//! current, and ask the client to request the tokens again with
//! `workspace/semanticTokens/refresh` when better facts exist. The server sends that request
//! only to a client that sets `workspace.semanticTokens.refreshSupport`. The refresh is
//! workspace-wide, so at most one refresh waits per document.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tower_lsp_server::ls_types::{ClientCapabilities, Uri};

/// Longest a semantic-tokens request waits for the analysis of the newest text before it
/// answers from the last facts carried over the edit.
pub const SEMANTIC_TOKENS_FACTS_WAIT: Duration = Duration::from_millis(100);

/// Whether the client accepts `workspace/semanticTokens/refresh`, and the documents that wait
/// for current facts to send one.
#[derive(Debug, Default)]
pub struct TokenRefresh {
    supported: AtomicBool,
    pending: Mutex<HashSet<String>>,
}

impl TokenRefresh {
    /// Record the refresh support of the client from its `initialize` capabilities.
    pub fn configure(&self, capabilities: &ClientCapabilities) {
        let supported = capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.semantic_tokens.as_ref())
            .and_then(|tokens| tokens.refresh_support)
            .unwrap_or(false);
        self.supported.store(supported, Ordering::Release);
    }

    /// `true` when the caller must wait for the current facts of `uri` and then send one
    /// refresh: the client supports refresh and no refresh waits for `uri` yet.
    pub fn claim(&self, uri: &Uri) -> bool {
        self.supported.load(Ordering::Acquire)
            && self.pending.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(uri.as_str().to_string())
    }

    /// The refresh for `uri` was sent (or given up); a later stale answer may claim again.
    pub fn release(&self, uri: &Uri) {
        self.pending.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(uri.as_str());
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{
        ClientCapabilities, SemanticTokensWorkspaceClientCapabilities, Uri, WorkspaceClientCapabilities,
    };

    use super::TokenRefresh;

    fn capabilities(refresh: Option<bool>) -> ClientCapabilities {
        ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                semantic_tokens: Some(SemanticTokensWorkspaceClientCapabilities { refresh_support: refresh }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_client_without_refresh_support_is_never_asked_to_refresh() {
        let uri = Uri::from_str("file:///w/A.bd").expect("uri");
        let refresh = TokenRefresh::default();
        assert!(!refresh.claim(&uri), "refresh is off before initialize");
        refresh.configure(&ClientCapabilities::default());
        assert!(!refresh.claim(&uri));
        refresh.configure(&capabilities(Some(false)));
        assert!(!refresh.claim(&uri));
    }

    #[test]
    fn one_refresh_waits_per_document_until_it_is_released() {
        let a = Uri::from_str("file:///w/A.bd").expect("uri");
        let b = Uri::from_str("file:///w/B.bd").expect("uri");
        let refresh = TokenRefresh::default();
        refresh.configure(&capabilities(Some(true)));
        assert!(refresh.claim(&a));
        assert!(!refresh.claim(&a), "a second stale answer for A joins the waiting refresh");
        assert!(refresh.claim(&b), "another document waits on its own");
        refresh.release(&a);
        assert!(refresh.claim(&a), "after the refresh a new stale answer waits again");
    }
}
