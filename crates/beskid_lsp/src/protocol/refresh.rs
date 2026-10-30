//! Refresh requests after an answer from facts that are not current.
//!
//! Semantic tokens and inlay hints answer within a short budget, from the last analyzed facts
//! carried over the edit when the analysis of the newest text is still running. LSP 3.16
//! (`workspace/semanticTokens/refresh`) and LSP 3.17 (`workspace/inlayHint/refresh`) let the
//! server then ask the client to request them again. The server sends a refresh only to a
//! client that sets the matching `refreshSupport` capability. A refresh is workspace-wide, so at
//! most one refresh of each kind waits per document.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tower_lsp_server::ls_types::{ClientCapabilities, Uri};

/// The request a client is asked to send again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshKind {
    /// `workspace/semanticTokens/refresh`.
    SemanticTokens,
    /// `workspace/inlayHint/refresh`.
    InlayHints,
}

/// Whether the client accepts one kind of refresh, and the documents that wait for current
/// facts to send one.
#[derive(Debug)]
pub struct FactsRefresh {
    kind: RefreshKind,
    supported: AtomicBool,
    pending: Mutex<HashSet<String>>,
}

impl FactsRefresh {
    /// Refresh of `kind`, off until [`FactsRefresh::configure`] reads the client capabilities.
    pub fn new(kind: RefreshKind) -> Self {
        Self { kind, supported: AtomicBool::new(false), pending: Mutex::new(HashSet::new()) }
    }

    pub fn kind(&self) -> RefreshKind {
        self.kind
    }

    /// Record the refresh support of the client from its `initialize` capabilities.
    pub fn configure(&self, capabilities: &ClientCapabilities) {
        let workspace = capabilities.workspace.as_ref();
        let supported = match self.kind {
            RefreshKind::SemanticTokens => workspace
                .and_then(|workspace| workspace.semantic_tokens.as_ref())
                .and_then(|tokens| tokens.refresh_support),
            RefreshKind::InlayHints => {
                workspace.and_then(|workspace| workspace.inlay_hint.as_ref()).and_then(|hints| hints.refresh_support)
            }
        }
        .unwrap_or(false);
        self.supported.store(supported, Ordering::Release);
    }

    /// `true` when the caller must wait for the current facts of `uri` and then send one
    /// refresh: the client supports refresh and no refresh of this kind waits for `uri` yet.
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
        ClientCapabilities, InlayHintWorkspaceClientCapabilities, SemanticTokensWorkspaceClientCapabilities, Uri,
        WorkspaceClientCapabilities,
    };

    use super::{FactsRefresh, RefreshKind};

    fn capabilities(tokens: Option<bool>, hints: Option<bool>) -> ClientCapabilities {
        ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                semantic_tokens: Some(SemanticTokensWorkspaceClientCapabilities { refresh_support: tokens }),
                inlay_hint: Some(InlayHintWorkspaceClientCapabilities { refresh_support: hints }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_client_without_refresh_support_is_never_asked_to_refresh() {
        let uri = Uri::from_str("file:///w/A.bd").expect("uri");
        for kind in [RefreshKind::SemanticTokens, RefreshKind::InlayHints] {
            let refresh = FactsRefresh::new(kind);
            assert!(!refresh.claim(&uri), "refresh is off before initialize");
            refresh.configure(&ClientCapabilities::default());
            assert!(!refresh.claim(&uri));
            refresh.configure(&capabilities(Some(false), Some(false)));
            assert!(!refresh.claim(&uri));
        }
    }

    #[test]
    fn each_kind_reads_its_own_capability() {
        let uri = Uri::from_str("file:///w/A.bd").expect("uri");
        let tokens = FactsRefresh::new(RefreshKind::SemanticTokens);
        let hints = FactsRefresh::new(RefreshKind::InlayHints);
        tokens.configure(&capabilities(None, Some(true)));
        hints.configure(&capabilities(None, Some(true)));
        assert!(!tokens.claim(&uri), "inlay-hint refresh support does not enable token refresh");
        assert!(hints.claim(&uri));
        tokens.configure(&capabilities(Some(true), None));
        hints.configure(&capabilities(Some(true), None));
        assert!(tokens.claim(&uri));
        hints.release(&uri);
        assert!(!hints.claim(&uri), "token refresh support does not enable inlay-hint refresh");
    }

    #[test]
    fn one_refresh_waits_per_document_until_it_is_released() {
        let a = Uri::from_str("file:///w/A.bd").expect("uri");
        let b = Uri::from_str("file:///w/B.bd").expect("uri");
        let refresh = FactsRefresh::new(RefreshKind::SemanticTokens);
        refresh.configure(&capabilities(Some(true), None));
        assert!(refresh.claim(&a));
        assert!(!refresh.claim(&a), "a second stale answer for A joins the waiting refresh");
        assert!(refresh.claim(&b), "another document waits on its own");
        refresh.release(&a);
        assert!(refresh.claim(&a), "after the refresh a new stale answer waits again");
    }
}
