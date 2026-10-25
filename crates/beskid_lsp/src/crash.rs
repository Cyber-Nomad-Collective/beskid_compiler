//! Crash isolation for the language server.
//!
//! A panic inside one request or notification handler must not end the session.
//! [`CrashReporter::guard`] runs a handler future under `catch_unwind`, turns a
//! panic into a value, writes at most one report per distinct panic location per
//! session to `~/.beskid/crash/lsp-<UTC time>.txt`, and tells the client once per
//! distinct crash through the `beskid/crashReport` notification.
//!
//! Reports carry the server version, the request method, the document path and a
//! backtrace. They never carry source text, and the panic message is left out
//! because messages such as string-slice failures can quote the document.
//!
//! Salsa cancellation unwinds with a [`salsa::Cancelled`] payload. That is a
//! normal outcome of a newer edit, not a crash, so it is reported separately and
//! never produces a report.

use std::any::Any;
use std::backtrace::Backtrace;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Once};
use std::thread::ThreadId;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tower_lsp_server::ls_types::notification::Notification;

/// Payload of the `beskid/crashReport` notification (camelCase on the wire).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportParams {
    /// Absolute path of the report file.
    pub report_path: String,
    /// One line describing the crash (method and panic location).
    pub summary: String,
    /// LSP method that was being handled.
    pub method: String,
}

/// The custom `beskid/crashReport` server-to-client notification.
pub enum CrashReport {}

impl Notification for CrashReport {
    type Params = CrashReportParams;
    const METHOD: &'static str = "beskid/crashReport";
}

/// What a guarded handler turned into after unwinding.
#[derive(Debug, PartialEq, Eq)]
pub enum Caught {
    /// Salsa cancelled the query because a newer input revision arrived.
    Cancelled,
    /// The handler panicked.
    Crashed {
        /// Report file, when one was written for this crash (first time at this location).
        report: Option<PathBuf>,
        /// Panic location as `file:line:column`.
        location: String,
    },
}

#[derive(Clone, Debug)]
struct PanicRecord {
    location: String,
    backtrace: String,
}

static RECORDS: Mutex<Option<HashMap<ThreadId, PanicRecord>>> = Mutex::new(None);
static HOOK: Once = Once::new();

/// Install the process panic hook that records the panic location and a backtrace.
/// Idempotent. The previous hook still runs, so stderr keeps its usual message.
pub fn install_panic_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown".to_string());
            let record = PanicRecord { location, backtrace: Backtrace::force_capture().to_string() };
            if let Ok(mut guard) = RECORDS.lock() {
                guard.get_or_insert_with(HashMap::new).insert(std::thread::current().id(), record.clone());
            }
            previous(info);
        }));
    });
}

/// The record of the panic that unwound on this thread. There is deliberately no global
/// fallback: a panic with no record of its own reports as `unknown` rather than borrowing
/// another request's location.
fn take_record() -> Option<PanicRecord> {
    RECORDS.lock().ok().and_then(|mut g| g.as_mut().and_then(|m| m.remove(&std::thread::current().id())))
}

/// A panic caught on a blocking worker thread, still carrying the identity of that thread.
pub struct BlockingPanic {
    thread: ThreadId,
    payload: Box<dyn Any + Send>,
}

impl BlockingPanic {
    /// Re-raise on the current (request) thread, handing over the worker thread's record so the
    /// guard attributes the crash to the panic's own location.
    pub fn resume(self) -> ! {
        let record = RECORDS.lock().ok().and_then(|mut g| g.as_mut().and_then(|m| m.remove(&self.thread)));
        if let Some(record) = record
            && let Ok(mut guard) = RECORDS.lock()
        {
            guard.get_or_insert_with(HashMap::new).insert(std::thread::current().id(), record);
        }
        std::panic::resume_unwind(self.payload)
    }
}

/// Run blocking work on a worker thread; a panic comes back as a [`BlockingPanic`].
pub fn run_blocking_recorded<T>(work: impl FnOnce() -> T) -> Result<T, BlockingPanic> {
    let thread = std::thread::current().id();
    std::panic::catch_unwind(AssertUnwindSafe(work)).map_err(|payload| BlockingPanic { thread, payload })
}

/// Boxed future returned by a crash notifier.
pub type NotifyFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type Notifier = Arc<dyn Fn(CrashReportParams) -> NotifyFuture + Send + Sync>;

/// Per-session crash isolation, reporting and de-duplication.
pub struct CrashReporter {
    dir: Option<PathBuf>,
    version: &'static str,
    seen: Mutex<HashSet<String>>,
    notifier: Mutex<Option<Notifier>>,
}

impl CrashReporter {
    /// Reporter writing to `dir` (`None` disables report files and the notification).
    pub fn new(dir: Option<PathBuf>) -> Self {
        install_panic_hook();
        Self { dir, version: env!("CARGO_PKG_VERSION"), seen: Mutex::new(HashSet::new()), notifier: Mutex::new(None) }
    }

    /// Reporter writing to `~/.beskid/crash`.
    pub fn for_home() -> Self {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        Self::new(home.map(|h| h.join(".beskid").join("crash")))
    }

    /// Set how crash notifications reach the client.
    pub fn set_notifier(&self, notifier: impl Fn(CrashReportParams) -> NotifyFuture + Send + Sync + 'static) {
        *self.notifier.lock().expect("crash notifier") = Some(Arc::new(notifier));
    }

    /// Run `future`; a panic or Salsa cancellation becomes an `Err` instead of unwinding further.
    pub async fn guard<T>(
        &self,
        method: &str,
        path: Option<&str>,
        future: impl Future<Output = T>,
    ) -> Result<T, Caught> {
        match AssertUnwindSafe(future).catch_unwind().await {
            Ok(value) => Ok(value),
            Err(payload) => Err(self.on_unwind(method, path, payload).await),
        }
    }

    /// Spawn a detached background task under the guard. A panic is reported like a handler
    /// panic and ends only that task; the next scheduling spawns a fresh one.
    pub fn spawn_guarded(
        self: &Arc<Self>,
        method: &'static str,
        path: Option<String>,
        future: impl Future<Output = ()> + Send + 'static,
    ) {
        let reporter = Arc::clone(self);
        tokio::spawn(async move {
            let _ = reporter.guard(method, path.as_deref(), future).await;
        });
    }

    /// Run a synchronous section (for example building a service future) under the same guard.
    pub async fn guard_sync<T>(&self, method: &str, path: Option<&str>, f: impl FnOnce() -> T) -> Result<T, Caught> {
        match std::panic::catch_unwind(AssertUnwindSafe(f)) {
            Ok(value) => Ok(value),
            Err(payload) => Err(self.on_unwind(method, path, payload).await),
        }
    }

    async fn on_unwind(&self, method: &str, path: Option<&str>, payload: Box<dyn Any + Send>) -> Caught {
        if payload.downcast_ref::<salsa::Cancelled>().is_some() {
            tracing::debug!(method, "request cancelled by a newer input revision");
            return Caught::Cancelled;
        }
        let record = take_record()
            .unwrap_or_else(|| PanicRecord { location: "unknown".into(), backtrace: "unavailable".into() });
        let first = self.seen.lock().expect("crash seen set").insert(record.location.clone());
        let mut report = None;
        if first {
            report = self.write_report(method, path, &record);
            if let Some(report_path) = &report {
                let params = CrashReportParams {
                    report_path: report_path.display().to_string(),
                    summary: format!("beskid_lsp crashed while handling `{method}` at {}", record.location),
                    method: method.to_string(),
                };
                let notifier = self.notifier.lock().expect("crash notifier").clone();
                if let Some(notify) = notifier {
                    notify(params).await;
                }
            }
        }
        tracing::error!(method, location = %record.location, "handler panicked; request failed, server continues");
        Caught::Crashed { report, location: record.location }
    }

    fn write_report(&self, method: &str, path: Option<&str>, record: &PanicRecord) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        std::fs::create_dir_all(dir).ok()?;
        let (stamp, human) = utc_stamp(SystemTime::now());
        let mut file = dir.join(format!("lsp-{stamp}.txt"));
        let mut counter = 1;
        while file.exists() {
            file = dir.join(format!("lsp-{stamp}-{counter}.txt"));
            counter += 1;
        }
        let body = format!(
            "beskid_lsp crash report\nversion: {}\ntime: {human}\nmethod: {method}\nfile: {}\npanic location: {}\n\nbacktrace:\n{}\n",
            self.version,
            path.unwrap_or("none"),
            record.location,
            record.backtrace,
        );
        std::fs::write(&file, body).ok()?;
        Some(file)
    }
}

/// File-name stamp (`20261024T153012Z`) and readable form for a UTC time.
fn utc_stamp(time: SystemTime) -> (String, String) {
    let secs = time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    (format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z"), format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    pub(crate) fn test_reporter(dir: &Path) -> (Arc<CrashReporter>, Arc<Mutex<Vec<CrashReportParams>>>) {
        let reporter = CrashReporter::new(Some(dir.to_path_buf()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&sent);
        reporter.set_notifier(move |params| {
            sink.lock().unwrap().push(params);
            Box::pin(async {})
        });
        (Arc::new(reporter), sent)
    }

    pub(crate) fn report_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir).map(|it| it.map(|e| e.unwrap().path()).collect()).unwrap_or_default()
    }

    async fn boom() -> u32 {
        panic!("secret source text");
    }

    #[test]
    fn stamp_formats_known_instant() {
        // 2026-10-24T15:40:00Z
        let (file, human) = utc_stamp(UNIX_EPOCH + Duration::from_secs(1_792_856_400));
        assert_eq!(file, "20261024T154000Z");
        assert_eq!(human, "2026-10-24T15:40:00Z");
    }

    #[tokio::test]
    async fn panic_becomes_error_and_next_call_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, _) = test_reporter(dir.path());
        let first = reporter.guard("textDocument/hover", Some("/p/A.bd"), boom()).await;
        assert!(matches!(first, Err(Caught::Crashed { .. })));
        assert_eq!(reporter.guard("textDocument/hover", Some("/p/A.bd"), async { 7 }).await, Ok(7));
    }

    #[tokio::test]
    async fn same_location_writes_one_report_and_one_notification() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, sent) = test_reporter(dir.path());
        for _ in 0..3 {
            let _ = reporter.guard("textDocument/hover", Some("/p/A.bd"), boom()).await;
        }
        let files = report_files(dir.path());
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("method: textDocument/hover"));
        assert!(text.contains("file: /p/A.bd"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(!text.contains("secret source text"), "report must not quote panic messages");
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].report_path, files[0].display().to_string());
        assert_eq!(sent[0].method, "textDocument/hover");
    }

    #[tokio::test]
    async fn panicking_background_task_produces_one_report_and_one_notification() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, sent) = test_reporter(dir.path());
        // Same call site twice, like a debounced task rescheduled after it died.
        for _ in 0..2 {
            reporter.spawn_guarded("textDocument/publishDiagnostics", Some("file:///p/A.bd".into()), async {
                panic!("analysis bug");
            });
            for _ in 0..50 {
                tokio::task::yield_now().await;
            }
        }
        let files = report_files(dir.path());
        assert_eq!(files.len(), 1);
        assert!(std::fs::read_to_string(&files[0]).unwrap().contains("method: textDocument/publishDiagnostics"));
        assert_eq!(sent.lock().unwrap().len(), 1);
        // The runtime and reporter keep working afterwards.
        assert_eq!(reporter.guard("x", None, async { 1 }).await, Ok(1));
    }

    #[tokio::test]
    async fn panic_without_its_own_record_is_not_attributed_to_an_earlier_one() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, sent) = test_reporter(dir.path());
        // A real panic on another thread leaves a record there, never on this one.
        let _ = std::thread::spawn(|| {
            let _ = std::panic::catch_unwind(|| panic!("elsewhere"));
        })
        .join();
        // A foreign unwind on this thread runs no panic hook, so there is nothing to attribute.
        let outcome = reporter
            .guard("x", None, async {
                if true {
                    std::panic::resume_unwind(Box::new("no hook"));
                }
            })
            .await;
        let Err(Caught::Crashed { location, .. }) = outcome else { panic!("crash expected") };
        assert_eq!(location, "unknown");
        assert_eq!(sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn blocking_worker_panic_keeps_its_own_location() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, _) = test_reporter(dir.path());
        let worker = tokio::task::spawn_blocking(|| run_blocking_recorded(|| -> u32 { panic!("worker bug") }))
            .await
            .expect("join");
        let outcome = reporter
            .guard("textDocument/completion", None, async move {
                match worker {
                    Ok(value) => value,
                    Err(panic) => panic.resume(),
                }
            })
            .await;
        let Err(Caught::Crashed { location, .. }) = outcome else { panic!("crash expected") };
        assert!(location.contains("crash.rs"), "attributed to the worker panic site: {location}");
    }

    #[tokio::test]
    async fn distinct_locations_report_separately() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, sent) = test_reporter(dir.path());
        let _ = reporter.guard("a", None, async { panic!("one") }).await;
        let _ = reporter.guard("b", None, async { panic!("two") }).await;
        assert_eq!(report_files(dir.path()).len(), 2);
        assert_eq!(sent.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn salsa_cancellation_is_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let (reporter, sent) = test_reporter(dir.path());
        let outcome = reporter
            .guard("textDocument/completion", None, async {
                if true {
                    std::panic::resume_unwind(Box::new(salsa::Cancelled::PendingWrite));
                }
            })
            .await;
        assert_eq!(outcome, Err::<(), _>(Caught::Cancelled));
        assert!(report_files(dir.path()).is_empty());
        assert!(sent.lock().unwrap().is_empty());
    }
}
