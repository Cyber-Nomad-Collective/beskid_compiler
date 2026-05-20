//! Fiber-aware mutex (parks waiting fibers; no poison in v1).

use std::sync::Mutex;

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::status::{MUTEX_OK, MUTEX_WOULD_BLOCK, STATUS_CANCELLED};

pub type MutexId = i64;

struct MutexInner {
    locked: bool,
    owner: Option<FiberKey>,
    waiters: Vec<FiberKey>,
}

static MUTEXES: Mutex<Option<slotmap::SlotMap<slotmap::DefaultKey, MutexInner>>> =
    Mutex::new(None);

fn mutex_table(
) -> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, MutexInner>>> {
    let mut guard = MUTEXES.lock().expect("mutex table lock");
    if guard.is_none() {
        *guard = Some(slotmap::SlotMap::with_key());
    }
    guard
}

fn key_to_id(key: slotmap::DefaultKey) -> MutexId {
    key.data().as_ffi() as i64
}

pub fn mutex_create() -> MutexId {
    let mut guard = mutex_table();
    let map = guard.as_mut().expect("mutex map");
    let key = map.insert(MutexInner {
        locked: false,
        owner: None,
        waiters: Vec::new(),
    });
    key_to_id(key)
}

pub fn mutex_lock(id: MutexId) -> i64 {
    if scheduler::current_fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    loop {
        let acquired = {
            let mut guard = mutex_table();
            let map = guard.as_mut().expect("mutex map");
            let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(id as u64));
            let Some(m) = map.get_mut(key) else {
                return STATUS_CANCELLED;
            };
            if !m.locked {
                m.locked = true;
                m.owner = scheduler::current_fiber_key();
                true
            } else {
                false
            }
        };
        if acquired {
            return MUTEX_OK;
        }
        scheduler::park_current(|f| {
            let _ = with_mutex(id, |m| {
                if !m.waiters.contains(&f) {
                    m.waiters.push(f);
                }
            });
        });
        if scheduler::current_fiber_cancelled() {
            return STATUS_CANCELLED;
        }
    }
}

pub fn mutex_try_lock(id: MutexId) -> i64 {
    if scheduler::current_fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    with_mutex(id, |m| {
        if m.locked {
            return MUTEX_WOULD_BLOCK;
        }
        m.locked = true;
        m.owner = scheduler::current_fiber_key();
        MUTEX_OK
    })
    .unwrap_or(MUTEX_WOULD_BLOCK)
}

pub fn mutex_unlock(id: MutexId) {
    let waiter = with_mutex(id, |m| {
        m.locked = false;
        m.owner = None;
        m.waiters.pop()
    })
    .flatten();
    if let Some(f) = waiter {
        scheduler::wake_fiber(f);
    }
}

fn with_mutex<F, R>(id: MutexId, f: F) -> Option<R>
where
    F: FnOnce(&mut MutexInner) -> R,
{
    let mut guard = mutex_table();
    let map = guard.as_mut()?;
    let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(id as u64));
    if !map.contains_key(key) {
        return None;
    }
    map.get_mut(key).map(f)
}
