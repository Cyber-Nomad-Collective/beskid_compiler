//! Homogeneous hub with round-robin `wait_receive` (max 256 registrations).

use std::cell::RefCell;

use slotmap::Key;

use crate::channel::{self, ChannelId};
use crate::scheduler;
use crate::slot_table::{LazySlotMap, lock_lazy_slot_map};
use crate::status::{STATUS_CANCELLED, STATUS_HUB_EMPTY, STATUS_HUB_LIMIT, STATUS_HUB_NOT_FOUND, STATUS_OK};

pub const HUB_MAX_REGISTRATIONS: usize = 256;

pub type HubId = i64;

#[derive(Clone)]
struct HubEntry {
    index: i64,
    channel: ChannelId,
}

struct HubInner {
    entries: Vec<HubEntry>,
    round_robin_cursor: usize,
}

static HUBS: LazySlotMap<HubInner> = LazySlotMap::new(None);

fn hubs() -> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, HubInner>>> {
    lock_lazy_slot_map(&HUBS, "hub table lock")
}

fn key_to_id(key: slotmap::DefaultKey) -> HubId {
    key.data().as_ffi() as i64
}

pub fn hub_create() -> HubId {
    let mut guard = hubs();
    let map = guard.as_mut().expect("hub map");
    let key = map.insert(HubInner { entries: Vec::new(), round_robin_cursor: 0 });
    key_to_id(key)
}

pub fn hub_register(hub_id: HubId, index: i64, channel_id: ChannelId) -> i64 {
    let mut guard = hubs();
    let map = guard.as_mut().expect("hub map");
    let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(hub_id as u64));
    let Some(hub) = map.get_mut(key) else {
        return STATUS_HUB_NOT_FOUND;
    };
    if hub.entries.len() >= HUB_MAX_REGISTRATIONS {
        return STATUS_HUB_LIMIT;
    }
    if let Some(pos) = hub.entries.iter().position(|e| e.index == index) {
        hub.entries[pos].channel = channel_id;
    } else {
        hub.entries.push(HubEntry { index, channel: channel_id });
    }
    STATUS_OK
}

pub fn hub_unregister(hub_id: HubId, index: i64) -> i64 {
    let mut guard = hubs();
    let map = guard.as_mut().expect("hub map");
    let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(hub_id as u64));
    let Some(hub) = map.get_mut(key) else {
        return STATUS_HUB_NOT_FOUND;
    };
    let before = hub.entries.len();
    hub.entries.retain(|e| e.index != index);
    if hub.entries.len() == before {
        return STATUS_HUB_NOT_FOUND;
    }
    STATUS_OK
}

thread_local! {
    static HUB_LAST_RECEIVE: RefCell<Option<(HubId, i64, i64)>> = const { RefCell::new(None) };
}

/// Parks until a member can satisfy `Receive`; follow with [`hub_wait_receive_index`] / [`hub_wait_receive_value`].
pub fn hub_wait_receive_status(hub_id: HubId) -> i64 {
    let mut index = 0i64;
    let mut value = 0i64;
    let status = hub_wait_receive(hub_id, &mut index, &mut value);
    if status == STATUS_OK {
        HUB_LAST_RECEIVE.with(|cell| *cell.borrow_mut() = Some((hub_id, index, value)));
    }
    status
}

pub fn hub_wait_receive_index(hub_id: HubId) -> i64 {
    HUB_LAST_RECEIVE.with(|cell| cell.borrow().and_then(|(id, index, _)| (id == hub_id).then_some(index))).unwrap_or(0)
}

pub fn hub_wait_receive_value(hub_id: HubId) -> i64 {
    HUB_LAST_RECEIVE.with(|cell| cell.borrow().and_then(|(id, _, value)| (id == hub_id).then_some(value))).unwrap_or(0)
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn hub_wait_receive(hub_id: HubId, out_index: *mut i64, out_value: *mut i64) -> i64 {
    if out_index.is_null() || out_value.is_null() {
        return STATUS_HUB_EMPTY;
    }
    if scheduler::current_fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    loop {
        let try_result = {
            let guard = hubs();
            let map = guard.as_ref().expect("hub map");
            let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(hub_id as u64));
            let Some(hub) = map.get(key) else {
                return STATUS_HUB_NOT_FOUND;
            };
            if hub.entries.is_empty() {
                return STATUS_HUB_EMPTY;
            }
            let len = hub.entries.len();
            let start = hub.round_robin_cursor % len;
            (start, len, hub.entries.clone(), hub.round_robin_cursor)
        };

        let (start, len, entries, _) = try_result;
        for offset in 0..len {
            let pos = (start + offset) % len;
            let entry = &entries[pos];
            let mut value = 0i64;
            let status = channel::channel_try_receive(entry.channel, &mut value);
            if status == STATUS_OK {
                // SAFETY: `out_index` and `out_value` were validated non-null on entry; the
                // caller passes stack-slot pointers that remain valid for the call duration.
                unsafe {
                    *out_index = entry.index;
                    *out_value = value;
                }
                let mut guard = hubs();
                let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(hub_id as u64));
                if let Some(hub) = guard.as_mut().and_then(|m| m.get_mut(key)) {
                    hub.round_robin_cursor = (pos + 1) % len.max(1);
                }
                return STATUS_OK;
            }
            if status == STATUS_CANCELLED {
                return STATUS_CANCELLED;
            }
        }

        scheduler::park_current(|_| {});
        if scheduler::current_fiber_cancelled() {
            return STATUS_CANCELLED;
        }
    }
}
