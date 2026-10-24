//! Scan scheduling: one workspace scan at a time, triggers coalesced, interactive work first.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Single-flight gate for workspace scans.
///
/// A trigger that arrives while a scan runs sets one `pending` flag instead of starting a
/// second scan; the running scan loop then performs exactly one follow-up round.
#[derive(Default)]
pub struct ScanCoordinator {
    flags: Mutex<Flags>,
}

#[derive(Default)]
struct Flags {
    running: bool,
    pending: bool,
}

impl ScanCoordinator {
    /// Register a scan trigger. `true`: the caller must run the scan loop. `false`: a scan is
    /// already running and has been told to run one more round.
    pub fn request(&self) -> bool {
        let mut flags = self.flags.lock().expect("scan flags");
        if flags.running {
            flags.pending = true;
            false
        } else {
            flags.running = true;
            true
        }
    }

    /// Called after a round. `true`: triggers arrived meanwhile, run one more round.
    pub fn finish_round(&self) -> bool {
        let mut flags = self.flags.lock().expect("scan flags");
        if flags.pending {
            flags.pending = false;
            true
        } else {
            flags.running = false;
            false
        }
    }

    /// Whether a scan loop is active.
    pub fn is_running(&self) -> bool {
        self.flags.lock().expect("scan flags").running
    }

    fn abort(&self) {
        let mut flags = self.flags.lock().expect("scan flags");
        flags.running = false;
        flags.pending = false;
    }
}

/// Releases the coordinator if the scan task dies (panic) before finishing normally.
pub struct ScanRunGuard {
    coordinator: Arc<ScanCoordinator>,
    armed: bool,
}

impl ScanRunGuard {
    pub fn new(coordinator: Arc<ScanCoordinator>) -> Self {
        Self { coordinator, armed: true }
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ScanRunGuard {
    fn drop(&mut self) {
        if self.armed {
            self.coordinator.abort();
        }
    }
}

/// Marks an interactive request (completion, edits) in flight so background scans yield to it.
pub struct InteractiveGuard(Arc<AtomicUsize>);

impl InteractiveGuard {
    pub fn new(counter: &Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(Arc::clone(counter))
    }
}

impl Drop for InteractiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Background scan work waits here between files while an interactive request is running.
pub async fn yield_to_interactive(counter: &AtomicUsize) {
    while counter.load(Ordering::Acquire) > 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Paths the server writes itself or must never analyze: build output, caches, VCS, dependencies.
pub fn is_ignored_watch_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component.as_os_str().to_str(), Some("obj" | ".beskid" | "target" | "node_modules" | ".git"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_during_a_scan_coalesce_into_one_follow_up_round() {
        let scan = ScanCoordinator::default();
        assert!(scan.request(), "first trigger starts the scan");
        for _ in 0..5 {
            assert!(!scan.request(), "triggers during a scan never start a second one");
        }
        assert!(scan.finish_round(), "all five triggers become exactly one follow-up round");
        assert!(!scan.finish_round(), "then the scan stops");
        assert!(!scan.is_running());
        assert!(scan.request(), "a later trigger starts a fresh scan");
    }

    #[test]
    fn dead_scan_task_releases_the_coordinator() {
        let scan = Arc::new(ScanCoordinator::default());
        assert!(scan.request());
        drop(ScanRunGuard::new(Arc::clone(&scan)));
        assert!(scan.request());
    }

    #[test]
    fn own_cache_and_build_output_paths_are_ignored() {
        for ignored in [
            "/w/p/obj/beskid/cache/salsa/db.bin",
            "/w/p/obj/beskid/materialized/Main.bd",
            "/w/p/.beskid/state.json",
            "/w/p/target/debug/x",
            "/w/p/obj/Project.lock",
        ] {
            assert!(is_ignored_watch_path(Path::new(ignored)), "{ignored}");
        }
        for watched in ["/w/p/Main.bd", "/w/p/App.bproj", "/w/p/Project.lock", "/w/p/Src/Util.bd"] {
            assert!(!is_ignored_watch_path(Path::new(watched)), "{watched}");
        }
    }

    #[tokio::test]
    async fn scan_yields_while_an_interactive_request_runs() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guard = InteractiveGuard::new(&counter);
        let waiting = tokio::spawn({
            let counter = Arc::clone(&counter);
            async move { yield_to_interactive(&counter).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished(), "scan work must wait for the interactive request");
        drop(guard);
        tokio::time::timeout(Duration::from_secs(1), waiting).await.expect("resumes").expect("task");
    }
}
