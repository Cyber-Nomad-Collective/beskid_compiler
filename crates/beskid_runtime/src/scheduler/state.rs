use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use corosensei::{Coroutine, CoroutineResult};
use slotmap::{Key, KeyData, SlotMap};

use crate::builtins::EventState;
use crate::fiber::Yielder;

use super::{spawn, tls};

slotmap::new_key_type! { pub struct FiberKey; }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FiberState {
    Runnable,
    Running,
    Parked,
    Done,
    Cancelled,
}

#[derive(Debug, Clone)]
pub(super) enum JoinOutcome {
    Value(i64),
    Cancelled,
    Panicked,
    #[allow(dead_code)]
    StackOverflow,
}

pub(super) struct Fiber {
    pub(super) state: FiberState,
    pub(super) coroutine: Option<Coroutine<(), (), i64>>,
    pub(super) entry: Option<extern "C" fn(*mut u8) -> i64>,
    pub(super) env: *mut u8,
    pub(super) detached: bool,
    pub(super) cancelled: AtomicBool,
    pub(super) outcome: Option<JoinOutcome>,
    pub(super) on_cancelled_slot: *mut *mut EventState,
    pub(super) _parent: Option<FiberKey>,
    pub(super) yielder: Option<*const Yielder<(), ()>>,
}

#[derive(Clone)]
pub(super) struct JoinSnapshot {
    pub(super) state: FiberState,
    pub(super) cancelled: bool,
    pub(super) outcome: Option<JoinOutcome>,
}

pub(super) struct Scheduler {
    pub(super) fibers: SlotMap<FiberKey, Fiber>,
    pub(super) run_queue: VecDeque<FiberKey>,
    pub(super) processor_count: usize,
    pub(super) clock_start: Instant,
    pub(super) main_fiber: FiberKey,
}

pub(super) fn id_to_key_unchecked(id: i64) -> FiberKey {
    FiberKey::from(KeyData::from_ffi(id as u64))
}

pub(super) fn key_to_id(key: FiberKey) -> i64 {
    key.data().as_ffi() as i64
}

impl Scheduler {
    pub(super) fn new() -> Self {
        let mut fibers = SlotMap::with_key();
        let main_fiber = fibers.insert(Fiber::empty());
        Self {
            fibers,
            run_queue: VecDeque::new(),
            processor_count: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            clock_start: Instant::now(),
            main_fiber,
        }
    }

    pub(super) fn spawn_fiber(
        &mut self,
        entry: extern "C" fn(*mut u8) -> i64,
        env: *mut u8,
        on_cancelled_slot: *mut *mut EventState,
        parent: Option<FiberKey>,
    ) -> FiberKey {
        let coroutine = Coroutine::new(move |yielder: &Yielder<(), ()>, ()| {
            run_fiber_body(yielder, entry, env)
        });
        let key = self.fibers.insert(Fiber {
            state: FiberState::Runnable,
            coroutine: Some(coroutine),
            entry: Some(entry),
            env,
            detached: false,
            cancelled: AtomicBool::new(false),
            outcome: None,
            on_cancelled_slot,
            _parent: parent,
            yielder: None,
        });
        self.run_queue.push_back(key);
        key
    }

    /// Run one scheduling step. Returns `true` when work was performed.
    pub(super) fn run_one_step(&mut self) -> bool {
        tls::apply_pending_states(self);
        spawn::apply_pending_spawns(self);
        spawn::apply_pending_detaches(self);
        spawn::apply_pending_cancels(self);
        tls::drain_pending_wakes(self);
        while let Some(next) = self.run_queue.pop_front() {
            tls::apply_pending_states(self);
            if self
                .fibers
                .get(next)
                .is_some_and(|f| f.state == FiberState::Parked)
            {
                continue;
            }
            self.resume_fiber(next);
            tls::drain_pending_wakes(self);
            tls::apply_pending_states(self);
            tls::refresh_join_snapshot(self);
            return true;
        }
        false
    }

    pub(super) fn should_continue(&self) -> bool {
        self.fibers.values().any(|f| {
            matches!(
                f.state,
                FiberState::Runnable | FiberState::Running | FiberState::Parked
            )
        })
    }

    pub(super) fn all_blocked(&self) -> bool {
        self.run_queue.is_empty()
            && self.fibers.values().all(|f| {
                matches!(
                    f.state,
                    FiberState::Parked | FiberState::Done | FiberState::Cancelled
                )
            })
    }

    pub(super) fn join_non_detached_children(&mut self) {
        let children: Vec<FiberKey> = self
            .fibers
            .iter()
            .filter(|(k, f)| {
                *k != self.main_fiber
                    && !f.detached
                    && !matches!(f.state, FiberState::Done | FiberState::Cancelled)
            })
            .map(|(k, _)| k)
            .collect();
        for key in children {
            let id = key_to_id(key);
            let mut out = 0i64;
            let _ = super::fiber_join(id, &mut out);
        }
    }

    fn resume_fiber(&mut self, key: FiberKey) {
        tls::apply_pending_states(self);
        let state = self.fibers.get(key).expect("fiber").state;
        if matches!(
            state,
            FiberState::Parked | FiberState::Done | FiberState::Cancelled
        ) {
            return;
        }
        let cancelled = self
            .fibers
            .get(key)
            .expect("fiber")
            .cancelled
            .load(Ordering::Acquire);
        self.fibers.get_mut(key).expect("fiber").state = FiberState::Running;
        tls::set_current(Some(key));
        tls::set_cancelled(cancelled);
        tls::set_current_cancelled(cancelled);
        tls::set_parking(false);
        if let Some(y) = self.fibers.get(key).and_then(|f| f.yielder) {
            tls::set_yielder(Some(y));
        }

        let result = {
            let coro = self
                .fibers
                .get_mut(key)
                .and_then(|f| f.coroutine.as_mut())
                .expect("coroutine");
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coro.resume(())))
        };

        tls::set_cancelled(false);
        tls::set_current_cancelled(false);
        tls::set_current(None);

        tls::apply_pending_states(self);
        let f = self.fibers.get_mut(key).expect("fiber");
        match result {
            Ok(CoroutineResult::Yield(())) => {
                f.yielder = tls::yielder();
                tls::set_yielder(None);
                let parked = tls::parking();
                tls::set_parking(false);
                if parked {
                    f.state = FiberState::Parked;
                } else if f.state != FiberState::Parked {
                    f.state = FiberState::Runnable;
                    if !self.run_queue.contains(&key) {
                        self.run_queue.push_back(key);
                    }
                }
            }
            Ok(CoroutineResult::Return(value)) => {
                let cancelled = f.cancelled.load(Ordering::Acquire);
                f.outcome = if cancelled {
                    Some(JoinOutcome::Cancelled)
                } else {
                    Some(JoinOutcome::Value(value))
                };
                f.state = if cancelled {
                    FiberState::Cancelled
                } else {
                    FiberState::Done
                };
                f.coroutine = None;
                f.yielder = None;
                tls::set_yielder(None);
                wake_all_parked_fibers(self);
            }
            Err(_) => {
                f.outcome = Some(JoinOutcome::Panicked);
                f.state = FiberState::Done;
                f.coroutine = None;
                f.yielder = None;
                tls::set_yielder(None);
            }
        }
    }
}

impl Fiber {
    fn empty() -> Self {
        Self {
            state: FiberState::Runnable,
            coroutine: None,
            entry: None,
            env: std::ptr::null_mut(),
            detached: false,
            cancelled: AtomicBool::new(false),
            outcome: None,
            on_cancelled_slot: std::ptr::null_mut(),
            _parent: None,
            yielder: None,
        }
    }

    pub(super) fn start(&mut self, entry: extern "C" fn(*mut u8) -> i64, env: *mut u8) {
        self.entry = Some(entry);
        self.env = env;
        self.coroutine = Some(Coroutine::new(move |yielder: &Yielder<(), ()>, ()| {
            run_fiber_body(yielder, entry, env)
        }));
        self.state = FiberState::Runnable;
    }
}

pub(super) fn wake_all_parked_fibers(s: &mut Scheduler) {
    let parked: Vec<FiberKey> = s
        .fibers
        .iter()
        .filter(|(_, f)| f.state == FiberState::Parked)
        .map(|(k, _)| k)
        .collect();
    for key in parked {
        wake_fiber_immediate(s, key);
    }
}

pub(super) fn wake_fiber_immediate(s: &mut Scheduler, key: FiberKey) {
    if let Some(f) = s.fibers.get_mut(key)
        && matches!(f.state, FiberState::Parked)
    {
        f.state = FiberState::Runnable;
        if !s.run_queue.contains(&key) {
            s.run_queue.push_back(key);
        }
    }
}

fn run_fiber_body(
    yielder: &Yielder<(), ()>,
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
) -> i64 {
    let yielder_ptr = yielder as *const Yielder<(), ()>;
    tls::set_yielder(Some(yielder_ptr));
    if let Some(key) = tls::current_fiber_key() {
        tls::try_with_scheduler(|s| {
            if let Some(f) = s.fibers.get_mut(key) {
                f.yielder = Some(yielder_ptr);
            }
        });
    }
    let value = entry(env);
    tls::set_yielder(None);
    value
}
