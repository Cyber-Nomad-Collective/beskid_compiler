//! Thread pool for blocking syscalls (Phase A: workers must not allocate without scheduler lock).

use std::sync::{Arc, Mutex, OnceLock};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use super::{park_current, wake_fiber, FiberKey};

struct SyscallJob {
    fiber: FiberKey,
    task: Box<dyn FnOnce() -> i64 + Send>,
    done_tx: mpsc::Sender<i64>,
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
pub fn run_blocking<F>(fiber: FiberKey, task: F) -> i64
where
    F: FnOnce() -> i64 + Send + 'static,
{
    let (done_tx, done_rx) = mpsc::channel();
    pool()
        .jobs
        .send(SyscallJob {
            fiber,
            task: Box::new(task),
            done_tx,
        })
        .expect("syscall job send");
    park_current(|_| {});
    done_rx.recv().unwrap_or(-1)
}
