//! [`TextSyncOrderedService`]: the request wrapper that keeps text-sync order and isolates
//! handler crashes.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};

use tokio::sync::Notify;
use tower_lsp_server::jsonrpc::{Error, Id, Request, Response};
use tower_service::Service;

use crate::crash::{Caught, CrashReporter};
use crate::features;
use crate::server::lifetime::{ExitHook, ExitState, watch_parent};

/// Preserve LSP text-sync order without serializing independent requests.
///
/// `tower-lsp-server` evaluates inbound messages with `buffer_unordered` inside its one
/// transport task, so a handler that computes synchronously would stop every other message,
/// reading and writing included. Each message therefore runs as its own runtime task. Tickets
/// are assigned here, synchronously in transport input order. A request waits only for earlier
/// text-sync notifications of its document; once that fence clears, peer requests execute
/// concurrently.
pub(crate) struct TextSyncOrderedService<S> {
    inner: Arc<tokio::sync::Mutex<S>>,
    next_text_sync: HashMap<String, u64>,
    completed_text_sync: Arc<Mutex<HashMap<String, u64>>>,
    text_sync_completed: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    reporter: Arc<CrashReporter>,
    /// Ends the process on `exit` and when the client process named by `initialize` is gone
    /// (see [`crate::server::lifetime`]); `None` in tests that drive the service in-process.
    exit: Option<ExitHook>,
    exit_state: Arc<ExitState>,
}

impl<S> TextSyncOrderedService<S> {
    pub(crate) fn new(inner: S, reporter: Arc<CrashReporter>) -> Self {
        Self {
            reporter,
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            next_text_sync: HashMap::new(),
            completed_text_sync: Arc::new(Mutex::new(HashMap::new())),
            text_sync_completed: Arc::new(Mutex::new(HashMap::new())),
            exit: None,
            exit_state: Arc::new(ExitState::default()),
        }
    }

    /// End the process through `exit` on the client's `exit` notification (code 0 after a
    /// `shutdown` request, else 1) and once the client process named by `initialize`'s
    /// `processId` is gone. In-flight work is dropped with the process.
    pub(crate) fn ending_the_process_with(mut self, exit: ExitHook) -> Self {
        self.exit = Some(exit);
        self
    }

    /// The protocol-level lifetime events of `request`: a `shutdown` request, the client's
    /// process id in `initialize`, and `exit`, which ends the process after the inner service
    /// saw it.
    fn observe_lifetime(&self, request: &Request) -> bool {
        match request.method() {
            "shutdown" => self.exit_state.shutdown_requested(),
            "initialize" => {
                if let (Some(exit), Some(pid)) = (
                    &self.exit,
                    request.params().and_then(|params| params.get("processId")).and_then(|pid| pid.as_u64()),
                ) {
                    watch_parent(pid as u32, Arc::clone(exit));
                }
            }
            "exit" => return true,
            _ => {}
        }
        false
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
        let exiting = self.observe_lifetime(&request);
        // Keep the open texts current in transport order before any handler runs.
        features::formatting::open_texts::observe(&request);
        let is_text_sync =
            matches!(request.method(), "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose");
        let ordering_key = text_document_key(&request);
        // Syntax-only requests read the open texts (already current here) and skip the fence.
        let required = if features::formatting::open_texts::is_syntax_only_request(request.method()) {
            0
        } else {
            ordering_key.as_ref().and_then(|key| self.next_text_sync.get(key)).copied().unwrap_or(0)
        };
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
        let exit = exiting.then(|| self.exit.clone()).flatten();
        let exit_code = self.exit_state.exit_code();
        let method = request.method().to_string();
        let request_id = request.id().cloned();
        let reply_id = request_id.clone();
        let path = request_path(&request);

        let received = std::time::Instant::now();
        crate::trace::event("recv", || format!("{method} id={request_id:?} uri={}", path.as_deref().unwrap_or("-")));
        let task = tokio::spawn(async move {
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
            let fence_ms = received.elapsed().as_millis();
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
            if let Some(exit) = exit {
                // `exit` was dispatched: the process ends now, whatever is still running.
                exit(exit_code);
            }
            crate::trace::event("done", || {
                format!(
                    "{method} id={request_id:?} fence_ms={fence_ms} total_ms={} uri={}",
                    received.elapsed().as_millis(),
                    path.as_deref().unwrap_or("-")
                )
            });
            result
        });
        Box::pin(async move {
            match task.await {
                Ok(result) => result,
                // The handler's own crash guard reports panics; this covers a task the runtime
                // dropped (shutdown) so a request still gets an answer.
                Err(error) => {
                    tracing::error!(%error, "LSP message task ended without a result");
                    Ok(reply_id.map(|id| Response::from_parts(id, Err(Error::internal_error()))))
                }
            }
        })
    }
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
                if request.method() == "test/computeBlocking" {
                    // As the server's compiler work does (`db_access::run_blocking_section`):
                    // the worker hands its other tasks to the pool before it blocks.
                    tokio::task::block_in_place(|| std::thread::sleep(std::time::Duration::from_millis(600)));
                }
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

    /// The transport polls every message future in one task; each message runs as its own
    /// task here, so a handler that blocks its worker in a blocking section (as the server's
    /// compiler work does) stops neither the transport nor the other messages. A handler that
    /// blocks a worker without a blocking section is a bug in that handler, not something
    /// this service can cure: with two workers the quick request may sit in the blocked
    /// worker's queue, which is why the former version of this test was flaky.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_handler_computing_in_a_blocking_section_does_not_stall_other_requests() {
        use futures_util::StreamExt;

        let inner = RecordingService {
            events: Arc::new(Mutex::new(Vec::new())),
            change_started: Arc::new(Notify::new()),
            release_change: Arc::new(Notify::new()),
        };
        let mut service = ordered(inner);
        let started = std::time::Instant::now();
        let mut in_flight = futures_util::stream::FuturesUnordered::new();
        in_flight.push(
            service.call(
                Request::build("test/computeBlocking")
                    .params(serde_json::json!({"textDocument": {"uri": "file:///tmp/Slow.bd"}}))
                    .id(1)
                    .finish(),
            ),
        );
        in_flight.push(
            service.call(
                Request::build("textDocument/hover")
                    .params(serde_json::json!({"textDocument": {"uri": "file:///tmp/Quick.bd"}}))
                    .id(2)
                    .finish(),
            ),
        );
        in_flight.next().await.expect("first message").expect("service");
        let first_done = started.elapsed();
        assert!(first_done < std::time::Duration::from_millis(400), "the quick request waited {first_done:?}");
        in_flight.next().await.expect("second message").expect("service");
    }

    #[tokio::test]
    async fn document_outline_does_not_wait_for_the_analysis_of_an_edit() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let change_started = Arc::new(Notify::new());
        let release_change = Arc::new(Notify::new());
        let inner = RecordingService {
            events: Arc::clone(&events),
            change_started: Arc::clone(&change_started),
            release_change: Arc::clone(&release_change),
        };
        let mut service = ordered(inner);
        let document = serde_json::json!({"textDocument": {"uri": "file:///tmp/Outline.bd"}});

        let change = service.call(Request::build("textDocument/didChange").params(document.clone()).finish());
        let outline = service.call(Request::build("textDocument/documentSymbol").params(document).id(1).finish());
        let change_task = tokio::spawn(change);
        let outline_task = tokio::spawn(outline);

        change_started.notified().await;
        tokio::time::timeout(std::time::Duration::from_millis(500), outline_task)
            .await
            .expect("the outline answers while the edit is still being analyzed")
            .expect("outline task")
            .expect("outline response");
        release_change.notify_waiters();
        change_task.await.expect("change task").expect("change response");
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
