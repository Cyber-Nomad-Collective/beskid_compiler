//! Thread pool for blocking syscalls.
//!
//! Phase A invariant: workers never act as Beskid mutators (no `alloc`, no channel pointer
//! sends, no GC writes). Phase B keeps that constraint by tagging every worker thread via
//! [`crate::gc::set_syscall_pool_worker`] so that any accidental allocation panics before it can
//! corrupt the heap. Workers that need to allocate (callback trampolines, foreign-thread
//! bridges, ...) must call [`crate::gc::enter_runtime_scope`] explicitly and own a scoped
//! [`crate::RuntimeRoot`].

use std::any::Any;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use crate::gc::set_syscall_pool_worker;

use super::{FiberKey, park_current, wake_fiber};

struct SyscallJob {
    fiber: FiberKey,
    task: Box<dyn FnOnce() -> Box<dyn Any + Send> + Send>,
    done_tx: mpsc::Sender<Box<dyn Any + Send>>,
}

struct PoolInner {
    jobs: SyncSender<SyscallJob>,
}

static POOL: OnceLock<PoolInner> = OnceLock::new();

fn pool() -> &'static PoolInner {
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<SyscallJob>(1024);
        let rx = Arc::new(Mutex::new(rx));
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get().max(2))
            .unwrap_or(2);
        for id in 0..worker_count {
            let rx = Arc::clone(&rx);
            thread::Builder::new()
                .name(format!("beskid-syscall-{id}"))
                .spawn(move || syscall_worker(rx))
                .expect("syscall worker spawn");
        }
        PoolInner { jobs: tx }
    })
}

fn syscall_worker(rx: Arc<Mutex<Receiver<SyscallJob>>>) {
    // Phase B guard: mark this OS thread so any accidental `alloc` or pointer-payload channel send
    // from inside a blocking syscall task panics with a descriptive runtime message rather than
    // silently re-entering the GC as a second mutator.
    set_syscall_pool_worker();
    loop {
        let job = match rx.lock().expect("syscall rx lock").recv() {
            Ok(job) => job,
            Err(_) => break,
        };
        let result = (job.task)();
        let _ = job.done_tx.send(result);
        wake_fiber(job.fiber);
    }
}

/// Run `task` on a pool thread and park the current fiber until it completes.
pub fn run_blocking_value<T, F>(fiber: FiberKey, task: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (done_tx, done_rx) = mpsc::channel();
    pool()
        .jobs
        .send(SyscallJob {
            fiber,
            task: Box::new(|| Box::new(task())),
            done_tx,
        })
        .expect("syscall job send");
    park_current(|_| {});
    *done_rx
        .recv()
        .expect("syscall job result")
        .downcast::<T>()
        .expect("syscall result type")
}

/// Run an integer-returning task on a pool thread and park the current fiber until it completes.
pub fn run_blocking<F>(fiber: FiberKey, task: F) -> i64
where
    F: FnOnce() -> i64 + Send + 'static,
{
    run_blocking_value(fiber, task)
}
