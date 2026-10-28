//! Scan scheduling: one workspace scan at a time, triggers coalesced, interactive work first.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

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

/// Count of interactive requests in flight, with a wake-up for waiters when it reaches zero.
#[derive(Default)]
pub struct InteractiveGate {
    active: AtomicUsize,
    idle: Notify,
}

impl InteractiveGate {
    /// Interactive requests currently running.
    pub fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

/// Marks an interactive request (completion, edits) in flight so background scans yield to it.
pub struct InteractiveGuard(Arc<InteractiveGate>);

impl InteractiveGuard {
    pub fn new(gate: &Arc<InteractiveGate>) -> Self {
        gate.active.fetch_add(1, Ordering::AcqRel);
        Self(Arc::clone(gate))
    }
}

impl Drop for InteractiveGuard {
    fn drop(&mut self) {
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

/// Longest a scan yields to interactive work before it takes one step anyway, so a steady
/// stream of requests cannot starve the scan.
pub const MAX_YIELD: Duration = Duration::from_millis(500);

/// Background scan work waits here between files while an interactive request is running. It
/// wakes the moment the last request ends, and gives up waiting after `max`.
pub async fn yield_to_interactive_for(gate: &InteractiveGate, max: Duration) {
    let wait = async {
        loop {
            let idle = gate.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if gate.active() == 0 {
                return;
            }
            idle.await;
        }
    };
    let _ = tokio::time::timeout(max, wait).await;
}

pub async fn yield_to_interactive(gate: &InteractiveGate) {
    yield_to_interactive_for(gate, MAX_YIELD).await;
}

/// Interactive analyses that may start while background work (the workspace index) waits for its
/// turn; the next one waits until the background step has started.
pub const INTERACTIVE_PER_BACKGROUND: usize = 2;

/// A fair share of analysis for background work. Interactive analyses go first, but once
/// [`INTERACTIVE_PER_BACKGROUND`] of them started while a background step was waiting, further
/// ones wait until that step has its database. A stream of edits can then delay the index by a
/// bounded amount of work, never stop it; requests never wait here (they answer from the last
/// facts while their analysis waits).
#[derive(Default)]
pub struct BackgroundTurn {
    waiting: AtomicUsize,
    interactive_since: AtomicUsize,
    turn: Notify,
}

impl BackgroundTurn {
    /// Called by an interactive analysis before it takes its permit.
    pub async fn interactive_turn(&self) {
        loop {
            let started = self.turn.notified();
            tokio::pin!(started);
            started.as_mut().enable();
            if self.waiting.load(Ordering::Acquire) == 0
                || self.interactive_since.load(Ordering::Acquire) < INTERACTIVE_PER_BACKGROUND
            {
                self.interactive_since.fetch_add(1, Ordering::AcqRel);
                return;
            }
            started.await;
        }
    }

    /// Called by a background step before it takes its permit; [`BackgroundWait::started`] once
    /// it holds its database.
    pub fn background_waiting(&self) -> BackgroundWait<'_> {
        if self.waiting.fetch_add(1, Ordering::AcqRel) == 0 {
            // The share counts the interactive analyses that started while this step waits.
            self.interactive_since.store(0, Ordering::Release);
        }
        BackgroundWait { turn: self, waiting: true }
    }

    fn release(&self) {
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        self.interactive_since.store(0, Ordering::Release);
        self.turn.notify_waiters();
    }
}

/// A background step waiting for its turn (released when it starts or is dropped).
pub struct BackgroundWait<'a> {
    turn: &'a BackgroundTurn,
    waiting: bool,
}

impl BackgroundWait<'_> {
    pub fn started(&mut self) {
        if std::mem::take(&mut self.waiting) {
            self.turn.release();
        }
    }
}

impl Drop for BackgroundWait<'_> {
    fn drop(&mut self) {
        self.started();
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
    async fn scan_wakes_the_moment_the_last_interactive_request_ends() {
        let gate = Arc::new(InteractiveGate::default());
        let first = InteractiveGuard::new(&gate);
        let second = InteractiveGuard::new(&gate);
        let waiting = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { yield_to_interactive_for(&gate, Duration::from_secs(30)).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished(), "scan work must wait while requests run");
        drop(first);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!waiting.is_finished(), "one request is still running");
        let released = std::time::Instant::now();
        drop(second);
        tokio::time::timeout(Duration::from_secs(1), waiting).await.expect("woken").expect("task");
        assert!(released.elapsed() < Duration::from_millis(100), "woken by notification, not by a poll interval");
    }

    #[tokio::test]
    async fn a_steady_stream_of_requests_cannot_starve_the_scan() {
        let gate = Arc::new(InteractiveGate::default());
        let _held = InteractiveGuard::new(&gate);
        let started = std::time::Instant::now();
        yield_to_interactive_for(&gate, Duration::from_millis(80)).await;
        assert!(started.elapsed() >= Duration::from_millis(70) && started.elapsed() < Duration::from_secs(1));
        assert_eq!(gate.active(), 1, "the request is still running; the scan proceeded after the bound");
    }

    #[tokio::test]
    async fn interactive_analyses_give_a_waiting_background_step_its_turn() {
        let turn = Arc::new(BackgroundTurn::default());
        for _ in 0..10 {
            tokio::time::timeout(Duration::from_millis(50), turn.interactive_turn())
                .await
                .expect("no background step waits: interactive analyses never wait");
        }
        let mut background = turn.background_waiting();
        for _ in 0..INTERACTIVE_PER_BACKGROUND {
            tokio::time::timeout(Duration::from_millis(50), turn.interactive_turn())
                .await
                .expect("interactive analyses go first up to the share");
        }
        let held = tokio::spawn({
            let turn = Arc::clone(&turn);
            async move { turn.interactive_turn().await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!held.is_finished(), "the next interactive analysis waits for the background step");
        background.started();
        tokio::time::timeout(Duration::from_secs(1), held).await.expect("released").expect("task");
    }

    #[tokio::test]
    async fn a_dropped_background_wait_releases_its_turn() {
        let turn = BackgroundTurn::default();
        drop(turn.background_waiting());
        for _ in 0..INTERACTIVE_PER_BACKGROUND + 2 {
            tokio::time::timeout(Duration::from_millis(50), turn.interactive_turn()).await.expect("no wait");
        }
    }

    #[tokio::test]
    async fn an_idle_gate_does_not_wait() {
        let gate = InteractiveGate::default();
        tokio::time::timeout(Duration::from_millis(20), yield_to_interactive(&gate)).await.expect("no wait");
    }
}
