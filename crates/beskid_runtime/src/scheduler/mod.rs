//! Cooperative M:N scheduler (Phase A: single GC mutator; fibers use [`corosensei`] stacks).

mod syscall_pool;

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use corosensei::{Coroutine, CoroutineResult};
use slotmap::{Key, KeyData, SlotMap};

use crate::builtins::{EventState, event_get_handler, event_len};
use crate::fiber::Yielder;
use crate::gc::with_current_root_if_active;
use crate::status::{
    FIBER_JOIN_CANCELLED, FIBER_JOIN_OK, FIBER_JOIN_PANICKED, FIBER_JOIN_STACK_OVERFLOW,
};

pub use syscall_pool::run_blocking;

slotmap::new_key_type! { pub struct FiberKey; }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FiberState {
    Runnable,
    Running,
    Parked,
    Done,
    Cancelled,
}

#[derive(Debug, Clone)]
enum JoinOutcome {
    Value(i64),
    Cancelled,
    Panicked,
    #[allow(dead_code)]
    StackOverflow,
}

struct Fiber {
    state: FiberState,
    coroutine: Option<Coroutine<(), (), i64>>,
    entry: Option<extern "C" fn(*mut u8) -> i64>,
    env: *mut u8,
    detached: bool,
    cancelled: AtomicBool,
    outcome: Option<JoinOutcome>,
    on_cancelled_slot: *mut *mut EventState,
    _parent: Option<FiberKey>,
    yielder: Option<*const Yielder<(), ()>>,
}

struct Scheduler {
    fibers: SlotMap<FiberKey, Fiber>,
    run_queue: VecDeque<FiberKey>,
    processor_count: usize,
    clock_start: Instant,
    main_fiber: FiberKey,
}

thread_local! {
    static SCHEDULER: std::cell::RefCell<Option<Scheduler>> = const { std::cell::RefCell::new(None) };
    static TLS_CURRENT: Cell<Option<FiberKey>> = const { Cell::new(None) };
    static TLS_IN_SCHEDULER: Cell<bool> = const { Cell::new(false) };
    static TLS_PARKING: Cell<bool> = const { Cell::new(false) };
    static TLS_YIELDER: Cell<Option<*const Yielder<(), ()>>> = const { Cell::new(None) };
    static TLS_CANCELLED: Cell<bool> = const { Cell::new(false) };
    /// Cancel bit for the currently running fiber; readable while the scheduler is borrowed.
    static TLS_CURRENT_CANCELLED: Cell<bool> = const { Cell::new(false) };
    static PENDING_WAKES: std::cell::RefCell<Vec<FiberKey>> = const { std::cell::RefCell::new(Vec::new()) };
    static PENDING_STATE: std::cell::RefCell<Vec<(FiberKey, FiberState)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static JOIN_SNAPSHOT: std::cell::RefCell<std::collections::HashMap<FiberKey, JoinSnapshot>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static PENDING_CANCEL: std::cell::RefCell<Vec<FiberKey>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PENDING_SPAWN: std::cell::RefCell<Vec<PendingSpawn>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PENDING_DETACH: std::cell::RefCell<Vec<FiberKey>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

struct PendingSpawn {
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
    on_cancelled_slot: *mut *mut EventState,
    parent: Option<FiberKey>,
}

thread_local! {
    static LAST_SPAWN_ID: Cell<i64> = const { Cell::new(0) };
}

fn apply_pending_spawns(s: &mut Scheduler) {
    let pending: Vec<PendingSpawn> = PENDING_SPAWN.with(|p| p.borrow_mut().drain(..).collect());
    for req in pending {
        let key = s.spawn_fiber(req.entry, req.env, req.on_cancelled_slot, req.parent);
        LAST_SPAWN_ID.with(|c| c.set(key_to_id(key)));
    }
}

fn apply_pending_detaches(s: &mut Scheduler) {
    let keys: Vec<FiberKey> = PENDING_DETACH.with(|p| p.borrow_mut().drain(..).collect());
    for key in keys {
        if let Some(f) = s.fibers.get_mut(key) {
            f.detached = true;
        }
    }
}

#[derive(Clone)]
struct JoinSnapshot {
    state: FiberState,
    cancelled: bool,
    outcome: Option<JoinOutcome>,
}

fn id_to_key_unchecked(id: i64) -> FiberKey {
    FiberKey::from(KeyData::from_ffi(id as u64))
}

fn refresh_join_snapshot(s: &Scheduler) {
    let mut map = std::collections::HashMap::new();
    for (key, fiber) in s.fibers.iter() {
        map.insert(
            key,
            JoinSnapshot {
                state: fiber.state,
                cancelled: fiber.cancelled.load(Ordering::Acquire),
                outcome: fiber.outcome.as_ref().map(|o| match o {
                    JoinOutcome::Value(v) => JoinOutcome::Value(*v),
                    JoinOutcome::Cancelled => JoinOutcome::Cancelled,
                    JoinOutcome::Panicked => JoinOutcome::Panicked,
                    JoinOutcome::StackOverflow => JoinOutcome::StackOverflow,
                }),
            },
        );
    }
    JOIN_SNAPSHOT.with(|snap| *snap.borrow_mut() = map);
}

fn join_status_from_snapshot(key: FiberKey, out: *mut i64) -> Option<i64> {
    JOIN_SNAPSHOT.with(|snap| {
        let snap = snap.borrow();
        let s = snap.get(&key)?;
        use FiberState::*;
        match s.state {
            Done | Cancelled => {
                if s.cancelled {
                    return Some(FIBER_JOIN_CANCELLED);
                }
                match s.outcome {
                    Some(JoinOutcome::Value(v)) => {
                        if !out.is_null() {
                            unsafe {
                                *out = v;
                            }
                        }
                        Some(FIBER_JOIN_OK)
                    }
                    Some(JoinOutcome::Panicked) => Some(FIBER_JOIN_PANICKED),
                    Some(JoinOutcome::StackOverflow) => Some(FIBER_JOIN_STACK_OVERFLOW),
                    Some(JoinOutcome::Cancelled) => Some(FIBER_JOIN_CANCELLED),
                    None => Some(FIBER_JOIN_PANICKED),
                }
            }
            _ => None,
        }
    })
}

fn apply_pending_cancels(s: &mut Scheduler) {
    let keys: Vec<FiberKey> = PENDING_CANCEL.with(|p| p.borrow_mut().drain(..).collect());
    for key in keys {
        if let Some(f) = s.fibers.get_mut(key) {
            f.cancelled.store(true, Ordering::Release);
            let slot = f.on_cancelled_slot;
            dispatch_on_cancelled(slot);
            if TLS_CURRENT.with(|c| c.get()) == Some(key) {
                TLS_CANCELLED.with(|c| c.set(true));
                TLS_CURRENT_CANCELLED.with(|c| c.set(true));
            }
            match f.state {
                FiberState::Parked => wake_fiber_immediate(s, key),
                FiberState::Runnable => {
                    if !s.run_queue.contains(&key) {
                        s.run_queue.push_back(key);
                    }
                }
                _ => {}
            }
        }
    }
}

fn set_fiber_state(key: FiberKey, state: FiberState) {
    let applied = SCHEDULER.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(s) = guard.as_mut()
            && let Some(f) = s.fibers.get_mut(key)
        {
            f.state = state;
            return true;
        }
        false
    });
    if !applied {
        PENDING_STATE.with(|p| p.borrow_mut().push((key, state)));
    }
}

fn apply_pending_states(s: &mut Scheduler) {
    let updates: Vec<(FiberKey, FiberState)> =
        PENDING_STATE.with(|p| p.borrow_mut().drain(..).collect());
    for (key, state) in updates {
        if let Some(f) = s.fibers.get_mut(key) {
            f.state = state;
        }
    }
}

fn with_scheduler<F, R>(f: F) -> R
where
    F: FnOnce(&mut Scheduler) -> R,
{
    SCHEDULER.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.is_none() {
            *guard = Some(Scheduler::new());
        }
        f(guard.as_mut().expect("scheduler"))
    })
}

pub fn init() {
    with_scheduler(|_| {});
}

pub fn in_fiber_scheduler() -> bool {
    TLS_IN_SCHEDULER.with(|v| v.get())
}

pub fn processor_count() -> usize {
    with_scheduler(|s| s.processor_count)
}

pub fn fiber_now_millis() -> i64 {
    with_scheduler(|s| {
        let elapsed = s.clock_start.elapsed().as_millis();
        elapsed.min(i64::MAX as u128) as i64
    })
}

pub fn current_fiber_id() -> i64 {
    TLS_CURRENT.with(|c| c.get()).map(key_to_id).unwrap_or(0)
}

pub fn current_fiber_key() -> Option<FiberKey> {
    TLS_CURRENT.with(|c| c.get())
}

pub fn current_fiber_cancelled() -> bool {
    TLS_CANCELLED.with(|c| c.get()) || TLS_CURRENT_CANCELLED.with(|c| c.get())
}

fn drain_pending_wakes(s: &mut Scheduler) {
    let pending: Vec<FiberKey> = PENDING_WAKES.with(|p| p.borrow_mut().drain(..).collect());
    for key in pending {
        wake_fiber_immediate(s, key);
    }
}

fn wake_all_parked_fibers(s: &mut Scheduler) {
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

fn wake_fiber_immediate(s: &mut Scheduler, key: FiberKey) {
    if let Some(f) = s.fibers.get_mut(key)
        && matches!(f.state, FiberState::Parked)
    {
        f.state = FiberState::Runnable;
        if !s.run_queue.contains(&key) {
            s.run_queue.push_back(key);
        }
    }
}

fn key_to_id(key: FiberKey) -> i64 {
    key.data().as_ffi() as i64
}

pub fn wake_fiber(key: FiberKey) {
    let woke = SCHEDULER.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(s) = guard.as_mut()
        {
            wake_fiber_immediate(s, key);
            return true;
        }
        false
    });
    if !woke {
        PENDING_WAKES.with(|p| p.borrow_mut().push(key));
    }
}

pub fn park_current(add_wait: impl FnOnce(FiberKey)) {
    let key = TLS_CURRENT.with(|c| c.get().expect("park outside fiber"));
    add_wait(key);
    set_fiber_state(key, FiberState::Parked);
    TLS_PARKING.with(|c| c.set(true));
    fiber_yield();
    TLS_PARKING.with(|c| c.set(false));
    set_fiber_state(key, FiberState::Running);
}

pub fn fiber_yield() {
    if !TLS_IN_SCHEDULER.with(|v| v.get()) {
        std::thread::yield_now();
        return;
    }
    if let Some(y) = TLS_YIELDER.with(|c| c.get()) {
        unsafe {
            (&*y).suspend(());
        }
    }
}

pub fn fiber_spawn(
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
    on_cancelled_slot: *mut *mut EventState,
) -> i64 {
    let parent = TLS_CURRENT.with(|c| c.get());
    let spawned = SCHEDULER.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(s) = guard.as_mut()
        {
            let key = s.spawn_fiber(entry, env, on_cancelled_slot, parent);
            return Some(key_to_id(key));
        }
        None
    });
    if let Some(id) = spawned {
        return id;
    }
    PENDING_SPAWN.with(|p| {
        p.borrow_mut().push(PendingSpawn {
            entry,
            env,
            on_cancelled_slot,
            parent,
        });
    });
    // Parent must yield so pending spawns are applied before using child id.
    fiber_yield();
    LAST_SPAWN_ID.with(|c| c.get())
}

pub fn fiber_detach(id: i64) {
    let key = id_to_key_unchecked(id);
    let applied = SCHEDULER.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(s) = guard.as_mut()
            && s.fibers.contains_key(key)
        {
            if let Some(f) = s.fibers.get_mut(key) {
                f.detached = true;
            }
            return true;
        }
        false
    });
    if !applied {
        PENDING_DETACH.with(|p| p.borrow_mut().push(key));
    }
}

pub fn fiber_cancel(id: i64) {
    let key = id_to_key_unchecked(id);
    PENDING_CANCEL.with(|p| p.borrow_mut().push(key));
    wake_fiber(key);
}

fn dispatch_on_cancelled(slot: *mut *mut EventState) {
    if slot.is_null() {
        return;
    }
    unsafe {
        let state_ptr = *slot;
        if state_ptr.is_null() {
            return;
        }
        let len = event_len(state_ptr);
        for idx in 0..len {
            let handler = event_get_handler(state_ptr, idx);
            if handler.is_null() {
                continue;
            }
            let callable: extern "C" fn() = std::mem::transmute(handler);
            if std::panic::catch_unwind(|| callable()).is_err() {
                panic!("unhandled panic in OnCancelled handler");
            }
        }
    }
}

pub fn fiber_join(id: i64, out: *mut i64) -> i64 {
    let key = id_to_key_unchecked(id);
    loop {
        if let Some(code) = join_status_from_snapshot(key, out) {
            return code;
        }
        park_current(|_| {});
    }
}

impl Scheduler {
    fn new() -> Self {
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

    fn spawn_fiber(
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
    fn run_one_step(&mut self) -> bool {
        apply_pending_states(self);
        apply_pending_spawns(self);
        apply_pending_detaches(self);
        apply_pending_cancels(self);
        drain_pending_wakes(self);
        while let Some(next) = self.run_queue.pop_front() {
            apply_pending_states(self);
            if self
                .fibers
                .get(next)
                .is_some_and(|f| f.state == FiberState::Parked)
            {
                continue;
            }
            self.resume_fiber(next);
            drain_pending_wakes(self);
            apply_pending_states(self);
            refresh_join_snapshot(self);
            return true;
        }
        false
    }

    fn should_continue(&self) -> bool {
        self.fibers.values().any(|f| {
            matches!(
                f.state,
                FiberState::Runnable | FiberState::Running | FiberState::Parked
            )
        })
    }

    fn all_blocked(&self) -> bool {
        self.run_queue.is_empty()
            && self.fibers.values().all(|f| {
                matches!(
                    f.state,
                    FiberState::Parked | FiberState::Done | FiberState::Cancelled
                )
            })
    }

    fn resume_fiber(&mut self, key: FiberKey) {
        apply_pending_states(self);
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
        TLS_CURRENT.with(|c| c.set(Some(key)));
        TLS_CANCELLED.with(|c| c.set(cancelled));
        TLS_CURRENT_CANCELLED.with(|c| c.set(cancelled));
        if let Some(y) = self.fibers.get(key).and_then(|f| f.yielder) {
            TLS_YIELDER.with(|c| c.set(Some(y)));
        }

        let result = {
            let coro = self
                .fibers
                .get_mut(key)
                .and_then(|f| f.coroutine.as_mut())
                .expect("coroutine");
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coro.resume(())))
        };

        TLS_CANCELLED.with(|c| c.set(false));
        TLS_CURRENT_CANCELLED.with(|c| c.set(false));
        TLS_CURRENT.with(|c| c.set(None));

        apply_pending_states(self);
        let f = self.fibers.get_mut(key).expect("fiber");
        match result {
            Ok(CoroutineResult::Yield(())) => {
                f.yielder = TLS_YIELDER.with(|c| c.get());
                TLS_YIELDER.with(|c| c.set(None));
                if TLS_PARKING.with(|c| c.get()) {
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
                TLS_YIELDER.with(|c| c.set(None));
                wake_all_parked_fibers(self);
            }
            Err(_) => {
                f.outcome = Some(JoinOutcome::Panicked);
                f.state = FiberState::Done;
                f.coroutine = None;
                f.yielder = None;
                TLS_YIELDER.with(|c| c.set(None));
            }
        }
    }

    fn join_non_detached_children(&mut self) {
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
            let _ = fiber_join(id, &mut out);
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

    fn start(&mut self, entry: extern "C" fn(*mut u8) -> i64, env: *mut u8) {
        self.entry = Some(entry);
        self.env = env;
        self.coroutine = Some(Coroutine::new(move |yielder: &Yielder<(), ()>, ()| {
            run_fiber_body(yielder, entry, env)
        }));
        self.state = FiberState::Runnable;
    }
}

fn run_fiber_body(
    yielder: &Yielder<(), ()>,
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
) -> i64 {
    let yielder_ptr = yielder as *const Yielder<(), ()>;
    TLS_YIELDER.with(|c| c.set(Some(yielder_ptr)));
    if let Some(key) = TLS_CURRENT.with(|c| c.get()) {
        SCHEDULER.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut()
                && let Some(s) = guard.as_mut()
                && let Some(f) = s.fibers.get_mut(key)
            {
                f.yielder = Some(yielder_ptr);
            }
        });
    }
    let value = entry(env);
    TLS_YIELDER.with(|c| c.set(None));
    value
}

/// Run `main` on fiber 0 and drive the scheduler until quiescence.
pub fn run_main_fiber(main: extern "C" fn(*mut u8) -> i64, env: *mut u8) -> i64 {
    SCHEDULER.with(|cell| {
        if let Some(s) = cell.borrow_mut().take() {
            std::mem::forget(s);
        }
    });
    let main_key = with_scheduler(|s| {
        let main_key = s.main_fiber;
        s.fibers.get_mut(main_key).expect("main").start(main, env);
        if !s.run_queue.contains(&main_key) {
            s.run_queue.push_back(main_key);
        }
        main_key
    });

    TLS_IN_SCHEDULER.with(|v| v.set(true));
    loop {
        let (continue_running, progressed, blocked) = with_scheduler(|s| {
            if !s.should_continue() {
                return (false, false, false);
            }
            let progressed = s.run_one_step();
            let blocked = !progressed && s.all_blocked();
            (true, progressed, blocked)
        });
        if !continue_running {
            break;
        }
        if !progressed {
            if blocked {
                with_scheduler(|s| {
                    if s.should_continue() {
                        panic!("scheduler deadlock: all fibers parked with an empty run queue");
                    }
                });
            }
            with_current_root_if_active(|root| {
                root.heap.collect();
                root.runtime_state.heap_live_bytes = root.heap.bytes_allocated();
            });
            std::thread::yield_now();
        }
    }
    TLS_IN_SCHEDULER.with(|v| v.set(false));

    let mut result = 0i64;
    with_scheduler(|s| {
        s.join_non_detached_children();
        if let Some(JoinOutcome::Value(n)) = s.fibers.get(main_key).and_then(|f| f.outcome.as_ref())
        {
            result = *n;
        }
        // Leak coroutine stacks on shutdown so Drop does not force-unwind across C unwind boundaries.
        for fiber in s.fibers.values_mut() {
            if let Some(coro) = fiber.coroutine.take() {
                std::mem::forget(coro);
            }
        }
    });
    SCHEDULER.with(|cell| {
        if let Some(s) = cell.borrow_mut().take() {
            std::mem::forget(s);
        }
    });
    result
}

/// Execute a Rust closure as the main fiber (for tests).
pub fn run_closure_as_main<F>(f: F) -> i64
where
    F: FnOnce() -> i64 + 'static,
{
    struct Ctx {
        func: Option<Box<dyn FnOnce() -> i64>>,
    }
    extern "C" fn trampoline(env: *mut u8) -> i64 {
        let ctx = unsafe { &mut *(env as *mut Ctx) };
        let func = ctx.func.take().expect("trampoline once");
        func()
    }
    let mut ctx = Ctx {
        func: Some(Box::new(f)),
    };
    let ptr = &mut ctx as *mut Ctx as *mut u8;
    run_main_fiber(trampoline, ptr)
}
