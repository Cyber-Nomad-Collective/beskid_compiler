use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::session::lifecycle::set_document;
use crate::session::project_context::invalidate_compilation_cache;
use crate::session::startup::signal_initial_scan_complete;
use crate::session::store::State;

fn uri() -> Uri {
    Uri::from_str("file:///concurrency_test.bd").expect("valid uri")
}

#[tokio::test]
async fn invalidate_and_set_document_do_not_panic_concurrently() {
    let state = Arc::new(RwLock::new(State::default()));
    let state_invalidate = state.clone();
    let state_document = state.clone();
    let file_uri = uri();
    let file_uri_for_document = file_uri.clone();
    let text = "i64 Main() { return 0; }".to_string();
    let expected_text = text.clone();

    let invalidate_task = tokio::spawn(async move {
        invalidate_compilation_cache(&state_invalidate).await;
    });
    let document_task = tokio::spawn(async move {
        signal_initial_scan_complete(&state_document).await;
        set_document(&state_document, file_uri_for_document, 1, text).await;
    });

    invalidate_task.await.expect("invalidate task");
    document_task.await.expect("document task");

    let read = state.read().await;
    let doc = read.docs.get(&file_uri).expect("document exists");
    assert_eq!(doc.version, 1);
    assert_eq!(doc.text, expected_text);
    assert!(
        doc.syntax_documentation
            .iter()
            .any(|fact| fact.name == "Main")
            || !doc.syntax_diagnostics.is_empty()
            || doc.syntax_completion.is_some()
            || !doc.syntax_symbols.is_empty(),
        "concurrent set_document must still bind syntax facts without ANALYSIS_CACHE_VERSION"
    );
}
