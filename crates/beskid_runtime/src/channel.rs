//! FIFO channel queues (pointer-sized payloads at ABI; unbounded by default).

use std::collections::VecDeque;
use std::sync::Mutex;

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::status::{STATUS_CANCELLED, STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK};

pub type ChannelId = i64;

struct ChannelInner {
    queue: VecDeque<i64>,
    capacity: Option<usize>,
    closed: bool,
    wait_senders: Vec<FiberKey>,
    wait_receivers: Vec<FiberKey>,
}

static CHANNELS: Mutex<Option<slotmap::SlotMap<slotmap::DefaultKey, ChannelInner>>> =
    Mutex::new(None);

fn channels() -> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, ChannelInner>>>
{
    let mut guard = CHANNELS.lock().expect("channel table lock");
    if guard.is_none() {
        *guard = Some(slotmap::SlotMap::with_key());
    }
    guard
}

fn key_to_id(key: slotmap::DefaultKey) -> ChannelId {
    key.data().as_ffi() as i64
}

/// Create a channel. `capacity <= 0` means unbounded; `capacity > 0` is bounded FIFO capacity.
pub fn channel_create(capacity: i64) -> ChannelId {
    let capacity = if capacity <= 0 {
        None
    } else {
        Some(capacity as usize)
    };
    let mut guard = channels();
    let map = guard.as_mut().expect("channel map");
    let key = map.insert(ChannelInner {
        queue: VecDeque::new(),
        capacity,
        closed: false,
        wait_senders: Vec::new(),
        wait_receivers: Vec::new(),
    });
    key_to_id(key)
}

fn with_channel<F, R>(id: ChannelId, f: F) -> Option<R>
where
    F: FnOnce(&mut ChannelInner) -> R,
{
    let mut guard = channels();
    let map = guard.as_mut()?;
    let key = slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(id as u64));
    if !map.contains_key(key) {
        return None;
    }
    map.get_mut(key).map(f)
}

fn fiber_cancelled() -> bool {
    scheduler::current_fiber_cancelled()
}

pub fn channel_send(id: ChannelId, value: i64) -> i64 {
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    let Some(status) = with_channel(id, |ch| {
        if ch.closed {
            return STATUS_CLOSED;
        }
        if let Some(cap) = ch.capacity
            && ch.queue.len() >= cap
        {
            scheduler::park_current(|f| {
                if !ch.wait_senders.contains(&f) {
                    ch.wait_senders.push(f);
                }
            });
            if fiber_cancelled() {
                return STATUS_CANCELLED;
            }
            if ch.closed {
                return STATUS_CLOSED;
            }
            if let Some(cap) = ch.capacity && ch.queue.len() >= cap {
                return STATUS_WOULD_BLOCK;
            }
        }
        ch.queue.push_back(value);
        if let Some(waiter) = ch.wait_receivers.pop() {
            scheduler::wake_fiber(waiter);
        }
        STATUS_OK
    }) else {
        return STATUS_CLOSED;
    };
    status
}

pub fn channel_try_send(id: ChannelId, value: i64) -> i64 {
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    with_channel(id, |ch| {
        if ch.closed {
            return STATUS_CLOSED;
        }
        if let Some(cap) = ch.capacity && ch.queue.len() >= cap {
            return STATUS_WOULD_BLOCK;
        }
        ch.queue.push_back(value);
        if let Some(waiter) = ch.wait_receivers.pop() {
            scheduler::wake_fiber(waiter);
        }
        STATUS_OK
    })
    .unwrap_or(STATUS_CLOSED)
}

/// Parks until a message is available (or the channel closes) without dequeuing.
pub fn channel_receive_status(id: ChannelId) -> i64 {
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    loop {
        let result = with_channel(id, |ch| {
            if !ch.queue.is_empty() {
                return Some(STATUS_OK);
            }
            if ch.closed {
                return Some(STATUS_CLOSED);
            }
            None
        });
        if let Some(Some(status)) = result {
            return status;
        }
        if result.is_none() {
            return STATUS_CLOSED;
        }
        scheduler::park_current(|f| {
            let _ = with_channel(id, |ch| {
                if !ch.wait_receivers.contains(&f) {
                    ch.wait_receivers.push(f);
                }
            });
        });
        if fiber_cancelled() {
            return STATUS_CANCELLED;
        }
    }
}

/// Dequeues one message after a successful [`channel_receive_status`] call.
pub fn channel_receive_value(id: ChannelId) -> i64 {
    let mut out = 0i64;
    let status = channel_receive(id, &mut out);
    if status == STATUS_OK {
        out
    } else {
        0
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)] // `out` validated non-null; JIT passes stack slots.
pub fn channel_receive(id: ChannelId, out: *mut i64) -> i64 {
    if out.is_null() {
        return STATUS_CLOSED;
    }
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    loop {
        let result = with_channel(id, |ch| {
            if let Some(value) = ch.queue.pop_front() {
                if let Some(waiter) = ch.wait_senders.pop() {
                    scheduler::wake_fiber(waiter);
                }
                return Some((STATUS_OK, value));
            }
            if ch.closed {
                return Some((STATUS_CLOSED, 0));
            }
            None
        });
        if let Some(Some((status, value))) = result {
            if status == STATUS_OK {
                unsafe {
                    *out = value;
                }
            }
            return status;
        }
        scheduler::park_current(|f| {
            let _ = with_channel(id, |ch| {
                if !ch.wait_receivers.contains(&f) {
                    ch.wait_receivers.push(f);
                }
            });
        });
        if fiber_cancelled() {
            return STATUS_CANCELLED;
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn channel_try_receive(id: ChannelId, out: *mut i64) -> i64 {
    if out.is_null() {
        return STATUS_CLOSED;
    }
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    with_channel(id, |ch| {
        if let Some(value) = ch.queue.pop_front() {
            if let Some(waiter) = ch.wait_senders.pop() {
                scheduler::wake_fiber(waiter);
            }
            unsafe {
                *out = value;
            }
            return STATUS_OK;
        }
        if ch.closed {
            return STATUS_CLOSED;
        }
        STATUS_WOULD_BLOCK
    })
    .unwrap_or(STATUS_CLOSED)
}

pub fn channel_close(id: ChannelId) {
    let waiters = with_channel(id, |ch| {
        ch.closed = true;
        let mut w = ch.wait_senders.clone();
        w.extend(ch.wait_receivers.iter().copied());
        w
    });
    if let Some(waiters) = waiters {
        for f in waiters {
            scheduler::wake_fiber(f);
        }
    }
}

/// Wake all parked send/receive waiters with cancellation (used by fiber cancel).
pub fn channel_cancel_waiters(id: ChannelId) {
    let waiters = with_channel(id, |ch| {
        let mut w = ch.wait_senders.clone();
        w.extend(ch.wait_receivers.iter().copied());
        ch.wait_senders.clear();
        ch.wait_receivers.clear();
        w
    });
    if let Some(waiters) = waiters {
        for f in waiters {
            scheduler::wake_fiber(f);
        }
    }
}
