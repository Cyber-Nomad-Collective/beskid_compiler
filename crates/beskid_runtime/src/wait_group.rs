//! WaitGroup counter with Add/Done/Wait operations.

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::slot_table::{LazySlotMap, lock_lazy_slot_map};
use crate::status::{STATUS_CANCELLED, STATUS_OK};

pub type WaitGroupId = i64;

struct WaitGroupInner {
    counter: i64,
    waiters: Vec<FiberKey>,
}

static WAIT_GROUPS: LazySlotMap<WaitGroupInner> = LazySlotMap::new(None);

fn table()
-> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, WaitGroupInner>>> {
    lock_lazy_slot_map(&WAIT_GROUPS, "wait_group table lock")
}
/// Convert a slotmap key to a wait group id.
/// @param(key) The slotmap key to convert.
/// @returns(WaitGroupId) The wait group id.
fn key_to_id(key: slotmap::DefaultKey) -> WaitGroupId {
    key.data().as_ffi() as i64
}

/// Create a wait group.
/// @returns(WaitGroupId) The wait group id.
pub fn wait_group_create() -> WaitGroupId {
    let mut guard = table();
    let map = guard.as_mut().expect("wait_group map");
    let key = map.insert(WaitGroupInner {
        counter: 0,
        waiters: Vec::new(),
    });
    key_to_id(key)
}

/// Add a delta to a wait group.
/// @param(id) The wait group id.
/// @param(delta) The delta to add.
pub fn wait_group_add(id: WaitGroupId, delta: i64) {
    let _ = with_wg(id, |wg| {
        wg.counter += delta;
    });
}

/// Decrement the counter of a wait group.
/// @param(id) The wait group id.
pub fn wait_group_done(id: WaitGroupId) {
    let wake = with_wg(id, |wg| {
        wg.counter -= 1;
        if wg.counter <= 0 {
            wg.counter = 0;
            std::mem::take(&mut wg.waiters)
        } else {
            Vec::new()
        }
    });
    if let Some(waiters) = wake {
        for f in waiters {
            scheduler::wake_fiber(f);
        }
    }
}

/// Wait for a wait group to be ready.
/// @param(id) The wait group id.
/// @returns(i64) The status of the wait.
pub fn wait_group_wait(id: WaitGroupId) -> i64 {
    if scheduler::current_fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    loop {
        let ready = with_wg(id, |wg| wg.counter <= 0).unwrap_or(true);
        if ready {
            return STATUS_OK;
        }
        scheduler::park_current(|f| {
            let _ = with_wg(id, |wg| {
                if !wg.waiters.contains(&f) {
                    wg.waiters.push(f);
                }
            });
        });
        if scheduler::current_fiber_cancelled() {
            return STATUS_CANCELLED;
        }
    }
}

/// Helper function to get a wait group inner.
/// @param(id) The wait group id.
/// @param(f) The function to call with the wait group inner.
/// @returns(Option<R>) The result of the function.
fn with_wg<F, R>(id: WaitGroupId, f: F) -> Option<R>
where
    F: FnOnce(&mut WaitGroupInner) -> R,
{
    let mut guard = table();
    let map = guard.as_mut()?;
    let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(id as u64));
    if !map.contains_key(key) {
        return None;
    }
    map.get_mut(key).map(f)
}
