//! WaitGroup counter with Add/Done/Wait operations.

use std::sync::Mutex;

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::status::{STATUS_CANCELLED, STATUS_OK};

pub type WaitGroupId = i64;

struct WaitGroupInner {
    counter: i64,
    waiters: Vec<FiberKey>,
}

static WAIT_GROUPS: Mutex<Option<slotmap::SlotMap<slotmap::DefaultKey, WaitGroupInner>>> =
    Mutex::new(None);

fn table()
-> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, WaitGroupInner>>> {
    let mut guard = WAIT_GROUPS.lock().expect("wait_group table lock");
    if guard.is_none() {
        *guard = Some(slotmap::SlotMap::with_key());
    }
    guard
}

fn key_to_id(key: slotmap::DefaultKey) -> WaitGroupId {
    key.data().as_ffi() as i64
}

pub fn wait_group_create() -> WaitGroupId {
    let mut guard = table();
    let map = guard.as_mut().expect("wait_group map");
    let key = map.insert(WaitGroupInner {
        counter: 0,
        waiters: Vec::new(),
    });
    key_to_id(key)
}

pub fn wait_group_add(id: WaitGroupId, delta: i64) {
    let _ = with_wg(id, |wg| {
        wg.counter += delta;
    });
}

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
