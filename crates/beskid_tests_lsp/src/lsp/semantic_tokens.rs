use beskid_lsp::server::backend::Backend;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::LspService;
use tower_lsp_server::ls_types::*;

use super::support::{open_document, open_sample_document, semantic_tokens_params, uri};

fn decoded_tokens(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut line = 0;
    let mut character = 0;
    tokens
        .iter()
        .map(|token| {
            line += token.delta_line;
            character = if token.delta_line == 0 { character + token.delta_start } else { token.delta_start };
            (line, character, token.length, token.token_type, token.token_modifiers_bitset)
        })
        .collect()
}

#[ignore = "requires initialized workspace scan + hydrated analysis"]
#[tokio::test]
async fn full_returns_highlights_for_open_document() {
    let (service, _socket) = LspService::new(Backend::new);
    let server = service.inner();
    let doc_uri = uri("file:///semantic_tokens_test.bd");
    open_sample_document(server, doc_uri.clone()).await;

    let response = server
        .semantic_tokens_full(semantic_tokens_params(doc_uri))
        .await
        .expect("semantic tokens request should succeed")
        .expect("semantic tokens should return result");

    let SemanticTokensResult::Tokens(tokens) = response else {
        panic!("expected full semantic tokens result");
    };

    assert!(!tokens.data.is_empty());
}

#[tokio::test]
async fn full_returns_none_after_document_close() {
    let (service, _socket) = LspService::new(Backend::new);
    let server = service.inner();
    let doc_uri = uri("file:///semantic_tokens_closed_test.bd");
    open_sample_document(server, doc_uri.clone()).await;

    server
        .did_close(DidCloseTextDocumentParams { text_document: TextDocumentIdentifier { uri: doc_uri.clone() } })
        .await;

    let response = server
        .semantic_tokens_full(semantic_tokens_params(doc_uri))
        .await
        .expect("semantic tokens request should succeed");

    assert!(response.is_none());
}

#[tokio::test]
async fn bsol_configuration_documents_emit_ast_backed_structural_tokens_for_every_extension() {
    const SOURCE: &str = "project {\n  name = \"demo\"\n  target \"app\" {\n    entry = \"main.bd\"\n  }\n}";
    const EXPECTED: &[(u32, u32, u32, u32, u32)] =
        &[(0, 0, 7, 5, 1), (1, 2, 4, 7, 1), (2, 2, 6, 5, 1), (3, 4, 5, 7, 1)];

    for extension in ["bproj", "bws", "bsol"] {
        let (service, _socket) = LspService::new(Backend::new);
        let server = service.inner();
        let doc_uri = uri(&format!("file:///semantic_tokens.{extension}"));
        open_document(server, doc_uri.clone(), "bsol", SOURCE.into()).await;

        let response = server
            .semantic_tokens_full(semantic_tokens_params(doc_uri))
            .await
            .expect("semantic tokens request should succeed")
            .expect("open BSOL document should return a result");
        let SemanticTokensResult::Tokens(tokens) = response else {
            panic!("expected full semantic tokens result");
        };

        assert_eq!(decoded_tokens(&tokens.data), EXPECTED, "unexpected BSOL token stream for .{extension}");
    }
}

#[tokio::test]
async fn invalid_bsol_fails_closed_without_semantic_tokens() {
    let (service, _socket) = LspService::new(Backend::new);
    let server = service.inner();
    let doc_uri = uri("file:///invalid-semantic-tokens.bsol");
    open_document(server, doc_uri.clone(), "bsol", "project { name = }".into()).await;

    let response = server
        .semantic_tokens_full(semantic_tokens_params(doc_uri))
        .await
        .expect("semantic tokens request should succeed")
        .expect("open BSOL document should return a result");
    let SemanticTokensResult::Tokens(tokens) = response else {
        panic!("expected full semantic tokens result");
    };

    assert!(tokens.data.is_empty(), "invalid BSOL must not publish partial semantic tokens");
}
