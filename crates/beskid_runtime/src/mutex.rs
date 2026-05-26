//! Fiber-aware mutex (parks waiting fibers; no poison in v1).

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::slot_table::{LazySlotMap, lock_lazy_slot_map};
use crate::status::{MUTEX_OK, MUTEX_WOULD_BLOCK, STATUS_CANCELLED};

pub type MutexId = i64;

struct MutexInner {
    locked: bool,
    owner: Option<FiberKey>,
    waiters: Vec<FiberKey>,
}

static MUTEXES: LazySlotMap<MutexInner> = LazySlotMap::new(None);

fn mutex_table()
-> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, MutexInner>>> {
    lock_lazy_slot_map(&MUTEXES, "mutex table lock")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{MUTEX_OK, MUTEX_WOULD_BLOCK};

    #[test]
    fn test_mutex_create_lock_unlock() {
        let mx = mutex_create();
        assert_ne!(mx, 0, "mutex id should be non-zero after creation");
        // SAFETY: mutex_try_lock is safe outside a fiber scheduler; it sets owner to
        // current_fiber_key() which returns None, and does not park.
        assert_eq!(mutex_try_lock(mx), MUTEX_OK);
        mutex_unlock(mx);
        // After unlock, try_lock should succeed again.
        assert_eq!(mutex_try_lock(mx), MUTEX_OK);
        mutex_unlock(mx);
    }

    #[test]
    fn test_mutex_try_lock_success() {
        let mx = mutex_create();
        assert_eq!(mutex_try_lock(mx), MUTEX_OK);
        // Locked mutex should report WOULD_BLOCK on a second try_lock.
        assert_eq!(mutex_try_lock(mx), MUTEX_WOULD_BLOCK);
        mutex_unlock(mx);
        assert_eq!(mutex_try_lock(mx), MUTEX_OK);
        mutex_unlock(mx);
    }
}
