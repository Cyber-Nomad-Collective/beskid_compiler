use std::str::FromStr;

use tower_lsp_server::ls_types::Uri;

use super::{rebuild_open_document_syntax_facts, set_document};
use crate::session::project_context::invalidate_compilation_cache;
use crate::session::store::{Document, State};

fn source() -> String {
    "i32 Main() { return 0; }".to_string()
}

fn uri() -> Uri {
    Uri::from_str("file:///cache_test.bd").expect("valid uri")
}

#[tokio::test]
async fn set_document_ignores_stale_versions() {
    let state = tokio::sync::RwLock::new(State::default());
    state.read().await.mark_initial_scan_complete();
    let file_uri = uri();
    set_document(&state, file_uri.clone(), 2, source()).await;
    set_document(&state, file_uri.clone(), 1, "i32 Main() { return 1; }".to_string()).await;

    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    assert_eq!(doc.version, 2);
    assert_eq!(doc.text, source());
}

#[tokio::test]
async fn hard_invalidation_clears_syntax_facts_until_rebuild() {
    let file_uri = uri();
    let state = tokio::sync::RwLock::new(State::default());
    state.read().await.mark_initial_scan_complete();
    set_document(&state, file_uri.clone(), 1, source()).await;
    {
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert!(
            doc.syntax_documentation.iter().any(|fact| fact.name == "Main"),
            "precondition: documentation facts bound"
        );
    }

    // Non-cold configured root so invalidate clears bound facts.
    {
        let mut write = state.write().await;
        write.configured_project_root = Some(std::path::PathBuf::from("/tmp/cyb78"));
    }

    invalidate_compilation_cache(&state).await;
    {
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert!(
            doc.syntax_documentation.is_empty() && doc.syntax_diagnostics.is_empty() && doc.syntax_completion.is_none(),
            "hard invalidation must fail closed without a shape-version cache"
        );
    }

    rebuild_open_document_syntax_facts(&state).await;
    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    assert!(
        doc.syntax_documentation.iter().any(|fact| fact.name == "Main"),
        "rebuild must rebind documentation facts to the current buffer"
    );
}

#[tokio::test]
async fn set_document_refreshes_documentation_facts_for_new_buffer_text() {
    let file_uri = uri();
    let state = tokio::sync::RwLock::new(State::default());
    state.read().await.mark_initial_scan_complete();
    set_document(&state, file_uri.clone(), 1, "i32 Old() { return 0; }".into()).await;
    {
        let read = state.read().await;
        let doc = read.docs.get(&file_uri).expect("document exists");
        assert!(doc.syntax_documentation.iter().any(|fact| fact.name == "Old"));
        assert!(!doc.syntax_documentation.iter().any(|fact| fact.name == "Current"));
    }
    set_document(&state, file_uri.clone(), 2, "i32 Current() { return 0; }".into()).await;
    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    assert!(
        doc.syntax_documentation.iter().any(|fact| fact.name == "Current"),
        "refresh must replace stale documentation facts"
    );
    assert!(!doc.syntax_documentation.iter().any(|fact| fact.name == "Old"));
}

#[tokio::test]
async fn set_document_binds_syntax_diagnostics_without_analysis_snapshot() {
    let file_uri = uri();
    let state = tokio::sync::RwLock::new(State::default());
    state.read().await.mark_initial_scan_complete();
    set_document(&state, file_uri.clone(), 1, source()).await;
    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    // Valid buffer: structural/prepare facts may be empty, but the field must be owned
    // by the Document revision (no Document.analysis snapshot).
    let _ = &doc.syntax_diagnostics;
    assert!(
        doc.syntax_diagnostics.iter().all(|diag| diag.code.as_deref() != Some("E1709")),
        "refresh must not attach orphaned composition diagnostics"
    );
}

#[tokio::test]
async fn set_document_rebuilds_same_text_after_cleared_facts() {
    let file_uri = uri();
    let text = source();
    let mut state = State::default();
    state.docs.insert(
        file_uri.clone(),
        Document {
            version: 1,
            text: text.clone(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        },
    );
    state.mark_initial_scan_complete();
    let state = tokio::sync::RwLock::new(state);
    set_document(&state, file_uri.clone(), 2, text).await;
    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    assert_eq!(doc.version, 2);
    assert!(
        doc.syntax_documentation.iter().any(|fact| fact.name == "Main"),
        "same-text upsert must rebuild facts after a cleared snapshot-free document"
    );
}
