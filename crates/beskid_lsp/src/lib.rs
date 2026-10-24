//! Beskid Language Server: document sync, semantic features, and workspace-wide indexing.

pub(crate) mod adapters;
pub(crate) mod commands;
pub mod crash;
pub(crate) mod diagnostics;
pub(crate) mod features;
pub(crate) mod logging;
pub(crate) mod manifest_uri;
pub(crate) mod position;
pub(crate) mod protocol;
pub(crate) mod scan_schedule;
pub mod server;
pub(crate) mod session;
pub(crate) mod standalone_bsol;
pub(crate) mod text_sync;
pub(crate) mod workspace_scan;
#[cfg(test)]
pub(crate) mod test_mod_host;

/// Serializes unit tests that share process-global state: the working directory
/// and the process-wide entry-session / typed-entry registries keyed by the
/// `beskid_e2e_tests/fixtures/corelib_mvp` entry. Each test owns a separate
/// `State`, but concurrent prepares of the same entry race on those registries
/// ("entry session syntax generation cannot regress"), which a real server
/// avoids through its single `db_gate`.
#[cfg(test)]
pub(crate) static SHARED_FIXTURE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

use crash::{Caught, CrashReporter};
use server::backend::Backend;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::sync::Notify;
use tower_lsp_server::jsonrpc::{Error, Id, Request, Response};
use tower_lsp_server::{LspService, Server};
use tower_service::Service;

/// Preserve LSP text-sync order without serializing independent requests.
///
/// `tower-lsp-server` intentionally evaluates inbound messages with
/// `buffer_unordered`. Tickets are assigned here, synchronously in transport
/// input order. A request waits only for earlier text-sync notifications; once
/// that fence clears, peer requests can execute concurrently.
struct TextSyncOrderedService<S> {
    inner: Arc<tokio::sync::Mutex<S>>,
    next_text_sync: HashMap<String, u64>,
    completed_text_sync: Arc<Mutex<HashMap<String, u64>>>,
    text_sync_completed: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    reporter: Arc<CrashReporter>,
}

impl<S> TextSyncOrderedService<S> {
    fn new(inner: S, reporter: Arc<CrashReporter>) -> Self {
        Self {
            reporter,
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            next_text_sync: HashMap::new(),
            completed_text_sync: Arc::new(Mutex::new(HashMap::new())),
            text_sync_completed: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

fn text_document_key(request: &Request) -> Option<String> {
    request
        .params()
        .and_then(|params| params.get("textDocument"))
        .and_then(|document| document.get("uri"))
        .and_then(|uri| uri.as_str())
        .map(str::to_owned)
        .or_else(|| request.method().starts_with("textDocument/").then(|| "<missing-uri>".to_string()))
}

/// Turn an isolated handler failure into the JSON-RPC reply (requests only).
fn failure_response(id: Option<Id>, method: &str, caught: &Caught) -> Option<Response> {
    let id = id?;
    let error = match caught {
        Caught::Cancelled => Error::content_modified(),
        Caught::Crashed { report, location } => {
            let mut error = Error::internal_error();
            error.message = match report {
                Some(path) => format!("internal error handling `{method}` (crash report: {})", path.display()).into(),
                None => format!("internal error handling `{method}` at {location}").into(),
            };
            error
        }
    };
    Some(Response::from_parts(id, Err(error)))
}

fn request_path(request: &Request) -> Option<String> {
    text_document_key(request).filter(|key| key != "<missing-uri>")
}

impl<S> Service<Request> for TextSyncOrderedService<S>
where
    S: Service<Request, Response = Option<Response>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = cx;
        // Readiness is checked again after the text-sync fence, immediately
        // before dispatch. Deferring it is necessary because the wrapped LSP
        // service is not cloneable and must not be called ahead of the fence.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let is_text_sync =
            matches!(request.method(), "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose");
        let ordering_key = text_document_key(&request);
        let required = ordering_key.as_ref().and_then(|key| self.next_text_sync.get(key)).copied().unwrap_or(0);
        let ticket = if is_text_sync {
            ordering_key.as_ref().map(|key| {
                let next = self.next_text_sync.entry(key.clone()).or_default();
                *next = next.saturating_add(1);
                *next
            })
        } else {
            None
        };
        let completed = Arc::clone(&self.completed_text_sync);
        let notify = ordering_key.as_ref().map(|key| {
            Arc::clone(
                self.text_sync_completed
                    .lock()
                    .expect("text sync notifier map")
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(Notify::new())),
            )
        });
        let inner = Arc::clone(&self.inner);
        let reporter = Arc::clone(&self.reporter);
        let method = request.method().to_string();
        let request_id = request.id().cloned();
        let path = request_path(&request);

        Box::pin(async move {
            while ordering_key.as_ref().is_some_and(|key| {
                completed.lock().expect("text sync completion map").get(key).copied().unwrap_or(0) < required
            }) {
                let notify = notify.as_ref().expect("ordered text-document request has notifier");
                let notified = notify.notified();
                if ordering_key.as_ref().is_some_and(|key| {
                    completed.lock().expect("text sync completion map").get(key).copied().unwrap_or(0) >= required
                }) {
                    break;
                }
                notified.await;
            }
            let result: Result<Option<Response>, S::Error> = async {
                let future = {
                    let mut inner = inner.lock().await;
                    std::future::poll_fn(|cx| inner.poll_ready(cx)).await?;
                    reporter.guard_sync(&method, path.as_deref(), || inner.call(request)).await
                };
                match future {
                    Ok(future) => match reporter.guard(&method, path.as_deref(), future).await {
                        Ok(result) => result,
                        Err(caught) => Ok(failure_response(request_id.clone(), &method, &caught)),
                    },
                    Err(caught) => Ok(failure_response(request_id.clone(), &method, &caught)),
                }
            }
            .await;
            if let (Some(key), Some(ticket), Some(notify)) = (ordering_key, ticket, notify) {
                completed.lock().expect("text sync completion map").insert(key, ticket);
                notify.notify_waiters();
            }
            result
        })
    }
}

/// Run the language server on stdio (used by `beskid_lsp` and `beskid dev lsp`).
pub async fn run_stdio_server() -> anyhow::Result<()> {
    // The LSP may be the first Beskid executable a user runs. Provision the
    // Corelib embedded in this binary before workspace resolution begins, so
    // editor diagnostics use the release bundle rather than a source checkout.
    beskid_tools::ensure_bundled_corelib(&beskid_corelib_embed::CORELIB_BUNDLE)?;
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let reporter = Arc::new(CrashReporter::for_home());
    let notifier_reporter = Arc::clone(&reporter);
    let (service, socket) = LspService::new(move |client| {
        let notify_client = client.clone();
        notifier_reporter.set_notifier(move |params| {
            let client = notify_client.clone();
            Box::pin(async move { client.send_notification::<crash::CrashReport>(params).await })
        });
        Backend::new(client).with_crash_reporter(Arc::clone(&notifier_reporter))
    });
    Server::new(stdin, stdout, socket).serve(TextSyncOrderedService::new(service, reporter)).await;
    Ok(())
}

#[cfg(test)]
mod text_sync_ordering_tests {
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};

    use tokio::sync::Notify;
    use tower_lsp_server::jsonrpc::{Request, Response};
    use tower_service::Service;

    use super::TextSyncOrderedService;
    use crate::crash::tests::{report_files, test_reporter};

    fn ordered<S>(inner: S) -> TextSyncOrderedService<S> {
        let dir = tempfile::tempdir().expect("crash dir").keep();
        TextSyncOrderedService::new(inner, test_reporter(&dir).0)
    }

    #[derive(Clone)]
    struct RecordingService {
        events: Arc<Mutex<Vec<&'static str>>>,
        change_started: Arc<Notify>,
        release_change: Arc<Notify>,
    }

    impl Service<Request> for RecordingService {
        type Response = Option<Response>;
        type Error = Infallible;
        type Future = std::pin::Pin<Box<dyn Future<Output = Result<Option<Response>, Infallible>> + Send>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: Request) -> Self::Future {
            let events = Arc::clone(&self.events);
            let change_started = Arc::clone(&self.change_started);
            let release_change = Arc::clone(&self.release_change);
            let is_change = request.method() == "textDocument/didChange";
            Box::pin(async move {
                events.lock().expect("event log").push(if is_change { "change-start" } else { "request-start" });
                if is_change {
                    change_started.notify_one();
                    release_change.notified().await;
                    events.lock().expect("event log").push("change-done");
                }
                if request.method() == "test/panic" {
                    panic!("handler bug");
                }
                Ok(None)
            })
        }
    }

    #[tokio::test]
    async fn later_request_waits_for_preceding_text_sync_notification() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let change_started = Arc::new(Notify::new());
        let release_change = Arc::new(Notify::new());
        let inner = RecordingService {
            events: Arc::clone(&events),
            change_started: Arc::clone(&change_started),
            release_change: Arc::clone(&release_change),
        };
        let mut service = ordered(inner);

        let change = service.call(Request::build("textDocument/didChange").finish());
        let completion = service.call(Request::build("textDocument/completion").id(1).finish());
        let change_task = tokio::spawn(change);
        let completion_task = tokio::spawn(completion);

        change_started.notified().await;
        assert_eq!(*events.lock().expect("event log"), vec!["change-start"]);
        release_change.notify_waiters();
        change_task.await.expect("change task").expect("change response");
        completion_task.await.expect("completion task").expect("completion response");
        assert_eq!(*events.lock().expect("event log"), vec!["change-start", "change-done", "request-start"]);
    }

    #[tokio::test]
    async fn unrelated_document_request_does_not_wait_for_text_sync() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let change_started = Arc::new(Notify::new());
        let release_change = Arc::new(Notify::new());
        let inner = RecordingService {
            events: Arc::clone(&events),
            change_started: Arc::clone(&change_started),
            release_change: Arc::clone(&release_change),
        };
        let mut service = ordered(inner);

        let change = service.call(
            Request::build("textDocument/didChange")
                .params(serde_json::json!({"textDocument": {"uri": "file:///tmp/First.bd"}}))
                .finish(),
        );
        let completion = service.call(
            Request::build("textDocument/completion")
                .params(serde_json::json!({"textDocument": {"uri": "file:///tmp/Second.bd"}}))
                .id(1)
                .finish(),
        );
        let change_task = tokio::spawn(change);
        let completion_task = tokio::spawn(completion);

        change_started.notified().await;
        tokio::time::timeout(std::time::Duration::from_millis(100), async {
            loop {
                if events.lock().expect("event log").contains(&"request-start") {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("unrelated request should run while the first document is changing");
        release_change.notify_waiters();
        change_task.await.expect("change task").expect("change response");
        completion_task.await.expect("completion task").expect("completion response");
    }

    #[tokio::test]
    async fn panicking_request_gets_error_response_and_server_keeps_serving() {
        let dir = tempfile::tempdir().expect("crash dir");
        let (reporter, sent) = test_reporter(dir.path());
        let inner = RecordingService {
            events: Arc::new(Mutex::new(Vec::new())),
            change_started: Arc::new(Notify::new()),
            release_change: Arc::new(Notify::new()),
        };
        let mut service = TextSyncOrderedService::new(inner, reporter);

        for id in 1..=2 {
            let reply = service
                .call(Request::build("test/panic").id(id).finish())
                .await
                .expect("service error")
                .expect("a request must get a response");
            let (_, body) = reply.into_parts();
            let error = body.expect_err("panic becomes a JSON-RPC error");
            assert_eq!(error.code, tower_lsp_server::jsonrpc::ErrorCode::InternalError);
        }
        // A panicking notification yields no reply and does not fail the service.
        assert!(service.call(Request::build("test/panic").finish()).await.expect("service error").is_none());

        let next = service.call(Request::build("textDocument/hover").id(3).finish()).await;
        assert!(next.expect("later request still served").is_none());

        assert_eq!(report_files(dir.path()).len(), 1, "one report per distinct panic location");
        assert_eq!(sent.lock().expect("sent").len(), 1, "one client notification per distinct crash");
    }
}
