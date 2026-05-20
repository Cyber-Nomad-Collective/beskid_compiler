use std::cell::{Cell, RefCell};
use std::sync::atomic::Ordering;

use crate::builtins::{EventState, event_get_handler, event_len};
use crate::channel;

use super::run_loop::fiber_yield;
use super::state::{FiberKey, FiberState, Scheduler, id_to_key_unchecked, key_to_id};
use super::tls;

struct PendingSpawn {
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
    on_cancelled_slot: *mut *mut EventState,
    parent: Option<FiberKey>,
}

thread_local! {
    static PENDING_CANCEL: RefCell<Vec<FiberKey>> = const { RefCell::new(Vec::new()) };
    static PENDING_SPAWN: RefCell<Vec<PendingSpawn>> = const { RefCell::new(Vec::new()) };
    static PENDING_DETACH: RefCell<Vec<FiberKey>> = const { RefCell::new(Vec::new()) };
    static LAST_SPAWN_ID: Cell<i64> = const { Cell::new(0) };
}

pub(super) fn apply_pending_spawns(s: &mut Scheduler) {
    let pending: Vec<PendingSpawn> = PENDING_SPAWN.with(|p| p.borrow_mut().drain(..).collect());
    for req in pending {
        let key = s.spawn_fiber(req.entry, req.env, req.on_cancelled_slot, req.parent);
        LAST_SPAWN_ID.with(|c| c.set(key_to_id(key)));
    }
}

pub(super) fn apply_pending_detaches(s: &mut Scheduler) {
    let keys: Vec<FiberKey> = PENDING_DETACH.with(|p| p.borrow_mut().drain(..).collect());
    for key in keys {
        if let Some(f) = s.fibers.get_mut(key) {
            f.detached = true;
        }
    }
}

pub(super) fn apply_pending_cancels(s: &mut Scheduler) {
    let keys: Vec<FiberKey> = PENDING_CANCEL.with(|p| p.borrow_mut().drain(..).collect());
    for key in keys {
        if let Some(f) = s.fibers.get_mut(key) {
            f.cancelled.store(true, Ordering::Release);
            channel::channel_cancel_waiter(key);
            let slot = f.on_cancelled_slot;
            dispatch_on_cancelled(slot);
            if tls::current_fiber_key() == Some(key) {
                tls::set_cancelled(true);
                tls::set_current_cancelled(true);
            }
            match f.state {
                FiberState::Parked => super::state::wake_fiber_immediate(s, key),
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

pub fn fiber_spawn(
    entry: extern "C" fn(*mut u8) -> i64,
    env: *mut u8,
    on_cancelled_slot: *mut *mut EventState,
) -> i64 {
    let parent = tls::current_fiber_key();
    let spawned = tls::try_with_scheduler(|s| {
        let key = s.spawn_fiber(entry, env, on_cancelled_slot, parent);
        key_to_id(key)
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
    let applied = tls::try_with_scheduler(|s| {
        if !s.fibers.contains_key(key) {
            return false;
        }
        if let Some(f) = s.fibers.get_mut(key) {
            f.detached = true;
        }
        true
    })
    .unwrap_or(false);
    if !applied {
        PENDING_DETACH.with(|p| p.borrow_mut().push(key));
    }
}

pub fn fiber_cancel(id: i64) {
    let key = id_to_key_unchecked(id);
    PENDING_CANCEL.with(|p| p.borrow_mut().push(key));
    tls::wake_fiber(key);
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
