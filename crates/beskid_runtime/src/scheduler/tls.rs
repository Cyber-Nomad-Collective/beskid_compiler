use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

use crate::fiber::Yielder;
use crate::status::{FIBER_JOIN_CANCELLED, FIBER_JOIN_OK, FIBER_JOIN_PANICKED};

use super::state::{FiberKey, FiberState, JoinOutcome, JoinSnapshot, Scheduler, key_to_id, wake_fiber_immediate};

thread_local! {
    static SCHEDULER: RefCell<Option<Scheduler>> = const { RefCell::new(None) };
    static TLS_CURRENT: Cell<Option<FiberKey>> = const { Cell::new(None) };
    static TLS_IN_SCHEDULER: Cell<bool> = const { Cell::new(false) };
    static TLS_PARKING: Cell<bool> = const { Cell::new(false) };
    static TLS_YIELDER: Cell<Option<*const Yielder<(), ()>>> = const { Cell::new(None) };
    static TLS_CANCELLED: Cell<bool> = const { Cell::new(false) };
    /// Cancel bit for the currently running fiber; readable while the scheduler is borrowed.
    static TLS_CURRENT_CANCELLED: Cell<bool> = const { Cell::new(false) };
    static PENDING_WAKES: RefCell<Vec<FiberKey>> = const { RefCell::new(Vec::new()) };
    static PENDING_STATE: RefCell<Vec<(FiberKey, FiberState)>> = const { RefCell::new(Vec::new()) };
    static JOIN_SNAPSHOT: RefCell<HashMap<FiberKey, JoinSnapshot>> = RefCell::new(HashMap::new());
}

pub(super) fn with_scheduler<F, R>(f: F) -> R
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

pub(super) fn try_with_scheduler<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Scheduler) -> R,
{
    SCHEDULER.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(s) = guard.as_mut()
        {
            return Some(f(s));
        }
        None
    })
}

pub(super) fn take_scheduler() -> Option<Scheduler> {
    SCHEDULER.with(|cell| cell.borrow_mut().take())
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
    current_fiber_key().map(key_to_id).unwrap_or(0)
}

pub fn current_fiber_key() -> Option<FiberKey> {
    TLS_CURRENT.with(|c| c.get())
}

pub fn current_fiber_cancelled() -> bool {
    TLS_CANCELLED.with(|c| c.get()) || TLS_CURRENT_CANCELLED.with(|c| c.get())
}

pub(super) fn set_current(key: Option<FiberKey>) {
    TLS_CURRENT.with(|c| c.set(key));
}

pub(super) fn set_in_scheduler(value: bool) {
    TLS_IN_SCHEDULER.with(|v| v.set(value));
}

pub(super) fn set_parking(value: bool) {
    TLS_PARKING.with(|c| c.set(value));
}

pub(super) fn parking() -> bool {
    TLS_PARKING.with(|c| c.get())
}

pub(super) fn set_yielder(value: Option<*const Yielder<(), ()>>) {
    TLS_YIELDER.with(|c| c.set(value));
}

pub(super) fn yielder() -> Option<*const Yielder<(), ()>> {
    TLS_YIELDER.with(|c| c.get())
}

pub(super) fn set_cancelled(value: bool) {
    TLS_CANCELLED.with(|c| c.set(value));
}

pub(super) fn set_current_cancelled(value: bool) {
    TLS_CURRENT_CANCELLED.with(|c| c.set(value));
}

pub(super) fn set_fiber_state(key: FiberKey, state: FiberState) {
    let applied = try_with_scheduler(|s| {
        if let Some(f) = s.fibers.get_mut(key) {
            f.state = state;
            return true;
        }
        false
    })
    .unwrap_or(false);
    if !applied {
        PENDING_STATE.with(|p| p.borrow_mut().push((key, state)));
    }
}

pub(super) fn apply_pending_states(s: &mut Scheduler) {
    let updates: Vec<(FiberKey, FiberState)> = PENDING_STATE.with(|p| p.borrow_mut().drain(..).collect());
    for (key, state) in updates {
        if let Some(f) = s.fibers.get_mut(key) {
            f.state = state;
        }
    }
}

pub(super) fn drain_pending_wakes(s: &mut Scheduler) {
    let pending: Vec<FiberKey> = PENDING_WAKES.with(|p| p.borrow_mut().drain(..).collect());
    for key in pending {
        wake_fiber_immediate(s, key);
    }
}

pub fn wake_fiber(key: FiberKey) {
    let woke = try_with_scheduler(|s| {
        wake_fiber_immediate(s, key);
        true
    })
    .unwrap_or(false);
    if !woke {
        PENDING_WAKES.with(|p| p.borrow_mut().push(key));
    }
}

pub(super) fn refresh_join_snapshot(s: &Scheduler) {
    let mut map = HashMap::new();
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
                }),
            },
        );
    }
    JOIN_SNAPSHOT.with(|snap| *snap.borrow_mut() = map);
}

pub(super) fn join_status_from_snapshot(key: FiberKey, out: *mut i64) -> Option<i64> {
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
                    Some(JoinOutcome::Cancelled) => Some(FIBER_JOIN_CANCELLED),
                    None => Some(FIBER_JOIN_PANICKED),
                }
            }
            _ => None,
        }
    })
}
