//! `textDocument/formatting` through the real request path: transport ordering service,
//! open-text mirror and backend, as format-on-save in the editor uses it. Text-sync notifications
//! are sent without waiting for their handlers, so the test also shows that formatting answers
//! without waiting for `didOpen` or analysis.

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::json;
use tower_lsp_server::LspService;
use tower_lsp_server::jsonrpc::{Request, Response};
use tower_lsp_server::ls_types::{TextDocumentContentChangeEvent, TextEdit};
use tower_service::Service;

use crate::server::backend::Backend;
use crate::text_sync::apply_document_changes;

const SOURCE: &str = "// Exercise header.\n// TODO: make it pass.\n\nuse Core.Io; // trailing on a use\n// Between items.\ni32 Answer() { return 42; } // trailing on a function\n\n// Before a test.\ntest answer_is_forty_two {\n    Testing.Assert.Equal(Answer(), 42); // trailing in a test\n}\n";

/// Send a request and wait for its response, failing instead of hanging.
async fn call<S>(service: &mut S, request: Request) -> Option<Response>
where
    S: Service<Request, Response = Option<Response>>,
    S::Error: std::fmt::Debug,
{
    std::future::poll_fn(|cx| service.poll_ready(cx)).await.expect("service ready");
    let method = request.method().to_string();
    tokio::time::timeout(Duration::from_secs(10), service.call(request))
        .await
        .unwrap_or_else(|_| panic!("`{method}` did not answer within 10 s"))
        .expect("service call")
}

/// Send a notification without waiting for its handler: formatting must not depend on it.
async fn notify<S>(service: &mut S, request: Request)
where
    S: Service<Request, Response = Option<Response>>,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug,
{
    std::future::poll_fn(|cx| service.poll_ready(cx)).await.expect("service ready");
    let future = service.call(request);
    tokio::spawn(async move {
        let _ = future.await;
    });
}

fn formatting_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/formatting")
        .params(json!({"textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": true}}))
        .id(id)
        .finish()
}

fn edits_of(response: Option<Response>) -> Vec<TextEdit> {
    let response = response.expect("formatting is a request and gets a response");
    let (_, body) = response.into_parts();
    serde_json::from_value(body.expect("formatting succeeds")).expect("a list of text edits")
}

fn comments(text: &str) -> Vec<&str> {
    text.lines().filter_map(|line| line.find("//").map(|index| line[index..].trim_end())).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn formatting_request_keeps_every_comment_and_then_needs_no_edits() {
    let (service, socket) = LspService::new(Backend::new);
    tokio::spawn(async move {
        let mut socket = socket;
        while socket.next().await.is_some() {}
    });
    let mut service = crate::TextSyncOrderedService::new(service);
    let uri = "file:///tmp/beskid-v067-fmt-request/Src/Main.bd";

    let init = Request::build("initialize").params(json!({"processId": null, "capabilities": {}})).id(1).finish();
    assert!(call(&mut service, init).await.expect("initialize response").error().is_none());
    notify(&mut service, Request::build("initialized").params(json!({})).finish()).await;
    notify(
        &mut service,
        Request::build("textDocument/didOpen")
            .params(json!({"textDocument": {"uri": uri, "languageId": "beskid", "version": 1, "text": SOURCE}}))
            .finish(),
    )
    .await;

    let edits = edits_of(call(&mut service, formatting_request(2, uri)).await);
    assert!(!edits.is_empty(), "the unformatted document needs an edit");
    let mut text = SOURCE.to_string();
    let changes = edits
        .into_iter()
        .map(|edit| TextDocumentContentChangeEvent { range: Some(edit.range), range_length: None, text: edit.new_text })
        .collect();
    apply_document_changes(&mut text, changes);
    for comment in comments(SOURCE) {
        assert!(text.contains(comment), "comment `{comment}` was lost by format-on-save:\n{text}");
    }
    assert!(text.starts_with("// Exercise header.\n// TODO: make it pass.\n\n"), "{text}");

    notify(
        &mut service,
        Request::build("textDocument/didChange")
            .params(json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": text}]}))
            .finish(),
    )
    .await;
    let again = edits_of(call(&mut service, formatting_request(3, uri)).await);
    assert!(again.is_empty(), "a formatted document must need 0 edits, got {again:?}");

    notify(&mut service, Request::build("textDocument/didClose").params(json!({"textDocument": {"uri": uri}})).finish())
        .await;
}
