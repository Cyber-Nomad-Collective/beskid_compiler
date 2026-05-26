//! FIFO channel queues (pointer-sized payloads at ABI; unbounded by default).

use std::collections::VecDeque;

use slotmap::Key;

use crate::scheduler::{self, FiberKey};
use crate::slot_table::{LazySlotMap, lock_lazy_slot_map};
use crate::status::{STATUS_CANCELLED, STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK};

pub type ChannelId = i64;

struct ChannelInner {
    queue: VecDeque<i64>,
    capacity: Option<usize>,
    closed: bool,
    wait_senders: Vec<FiberKey>,
    wait_receivers: Vec<FiberKey>,
}

static CHANNELS: LazySlotMap<ChannelInner> = LazySlotMap::new(None);

fn channels()
-> std::sync::MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, ChannelInner>>> {
    lock_lazy_slot_map(&CHANNELS, "channel table lock")
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
            if let Some(cap) = ch.capacity
                && ch.queue.len() >= cap
            {
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
        if let Some(cap) = ch.capacity
            && ch.queue.len() >= cap
        {
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
    if status == STATUS_OK { out } else { 0 }
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
                // SAFETY: `out` was validated non-null on entry; JIT and generated callers pass
                // stack-slot pointers that remain valid for the duration of the call.
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
            // SAFETY: `out` was validated non-null on entry; JIT and generated callers pass
            // stack-slot pointers that remain valid for the duration of the call.
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

/// Phase B pointer-payload send: register `value_ptr` as an external GC handle, push the handle
/// onto the channel as an `i64`, and apply the insertion write barrier so concurrent marking
/// retains the pointer reachable through the channel queue.
///
/// When called from a Beskid fiber, blocking semantics defer to the fiber scheduler. When called
/// from a Phase B mutator that lives on a raw OS thread (no fiber context), back-pressure on a
/// bounded queue is handled with a short `thread::yield_now` spin-poll rather than parking the
/// nonexistent fiber.
pub fn channel_send_ptr(id: ChannelId, value_ptr: *mut u8) -> i64 {
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    let handle =
        crate::gc::with_current_root_if_active(|root| crate::gc::store_handle(root, value_ptr));
    let Some(handle) = handle else {
        return STATUS_CLOSED;
    };
    crate::gc::with_current_heap(|heap| heap.write_barrier(std::ptr::null_mut(), value_ptr));
    let status = if scheduler::in_fiber_scheduler() {
        channel_send(id, handle as i64)
    } else {
        // OS-thread mutator: poll with `try_send` until the channel accepts or closes. Without a
        // fiber, the standard channel parking path would panic ("park outside fiber").
        loop {
            let s = channel_try_send(id, handle as i64);
            if s != STATUS_WOULD_BLOCK {
                break s;
            }
            std::thread::yield_now();
        }
    };
    if status != STATUS_OK {
        crate::gc::with_current_root_if_active(|root| crate::gc::drop_handle(root, handle));
    }
    status
}

/// Phase B pointer-payload try-send variant; see [`channel_send_ptr`].
pub fn channel_try_send_ptr(id: ChannelId, value_ptr: *mut u8) -> i64 {
    if fiber_cancelled() {
        return STATUS_CANCELLED;
    }
    let handle =
        crate::gc::with_current_root_if_active(|root| crate::gc::store_handle(root, value_ptr));
    let Some(handle) = handle else {
        return STATUS_CLOSED;
    };
    crate::gc::with_current_heap(|heap| heap.write_barrier(std::ptr::null_mut(), value_ptr));
    let status = channel_try_send(id, handle as i64);
    if status != STATUS_OK {
        crate::gc::with_current_root_if_active(|root| crate::gc::drop_handle(root, handle));
    }
    status
}

/// Phase B pointer-payload receive: dequeue an external GC handle, resolve it to the original
/// pointer, drop the handle, and apply the insertion write barrier on the receiver side so the
/// pointer is grayed for the receiver's mutator view.
///
/// OS-thread Phase B mutators poll with `try_receive` plus `thread::yield_now` rather than
/// parking, mirroring [`channel_send_ptr`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn channel_receive_ptr(id: ChannelId, out_ptr: *mut *mut u8) -> i64 {
    if out_ptr.is_null() {
        return STATUS_CLOSED;
    }
    let mut handle_slot = 0i64;
    let status = if scheduler::in_fiber_scheduler() {
        channel_receive(id, &mut handle_slot)
    } else {
        loop {
            let s = channel_try_receive(id, &mut handle_slot);
            if s != STATUS_WOULD_BLOCK {
                break s;
            }
            std::thread::yield_now();
        }
    };
    if status != STATUS_OK {
        return status;
    }
    let handle = handle_slot as u64;
    let value_ptr = crate::gc::with_current_heap(|heap| heap.external_roots().get_handle(handle))
        .unwrap_or(std::ptr::null_mut());
    crate::gc::with_current_root_if_active(|root| crate::gc::drop_handle(root, handle));
    crate::gc::with_current_heap(|heap| heap.write_barrier(std::ptr::null_mut(), value_ptr));
    // SAFETY: `out_ptr` was validated non-null on entry; the caller-provided pointer storage
    // lives for the duration of the call and is properly aligned for `*mut u8`.
    unsafe {
        *out_ptr = value_ptr;
    }
    STATUS_OK
}

/// Phase B pointer-payload try-receive variant; see [`channel_receive_ptr`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn channel_try_receive_ptr(id: ChannelId, out_ptr: *mut *mut u8) -> i64 {
    if out_ptr.is_null() {
        return STATUS_CLOSED;
    }
    let mut handle_slot = 0i64;
    let status = channel_try_receive(id, &mut handle_slot);
    if status != STATUS_OK {
        return status;
    }
    let handle = handle_slot as u64;
    let value_ptr = crate::gc::with_current_heap(|heap| heap.external_roots().get_handle(handle))
        .unwrap_or(std::ptr::null_mut());
    crate::gc::with_current_root_if_active(|root| crate::gc::drop_handle(root, handle));
    crate::gc::with_current_heap(|heap| heap.write_barrier(std::ptr::null_mut(), value_ptr));
    // SAFETY: `out_ptr` was validated non-null on entry; the caller-provided pointer storage
    // lives for the duration of the call and is properly aligned for `*mut u8`.
    unsafe {
        *out_ptr = value_ptr;
    }
    STATUS_OK
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK};

    #[test]
    fn test_channel_create_and_close() {
        let ch = channel_create(0);
        assert_ne!(ch, 0, "channel id should be non-zero after creation");
        channel_close(ch);
        assert_eq!(
            channel_try_send(ch, 1),
            STATUS_CLOSED,
            "try_send on closed channel should return STATUS_CLOSED"
        );
    }

    #[test]
    fn test_channel_send_receive_i64() {
        // SAFETY: Using an unbounded channel (capacity 0) so channel_send does not attempt to
        // park the calling fiber outside a scheduler context. The queue is non-empty when
        // channel_receive is called, so it also skips parking and returns immediately.
        let ch = channel_create(0);
        assert_eq!(channel_send(ch, 42), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 42);
        channel_close(ch);
    }

    #[test]
    fn test_channel_try_send_receive() {
        let ch = channel_create(0);
        assert_eq!(channel_try_send(ch, 100), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 100);
        channel_close(ch);
    }

    #[test]
    fn test_channel_close_behavior() {
        let ch = channel_create(0);
        channel_close(ch);
        assert_eq!(channel_try_send(ch, 1), STATUS_CLOSED);
        let mut out = 0i64;
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_CLOSED);
        // Closing an already-closed channel should not panic.
        channel_close(ch);
    }

    #[test]
    fn test_channel_bounded_try_would_block() {
        let ch = channel_create(1);
        assert_eq!(channel_try_send(ch, 10), STATUS_OK);
        assert_eq!(channel_try_send(ch, 11), STATUS_WOULD_BLOCK);
        let mut out = 0i64;
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 10);
        // After draining, sending should succeed again.
        assert_eq!(channel_try_send(ch, 20), STATUS_OK);
        channel_close(ch);
    }
}

/// Remove `waiter` from every channel wait list and wake it if it was parked on a channel.
pub fn channel_cancel_waiter(waiter: FiberKey) {
    let mut guard = channels();
    let Some(map) = guard.as_mut() else {
        return;
    };
    let mut removed = false;
    for ch in map.values_mut() {
        let before_senders = ch.wait_senders.len();
        ch.wait_senders.retain(|f| *f != waiter);
        let before_receivers = ch.wait_receivers.len();
        ch.wait_receivers.retain(|f| *f != waiter);
        removed |= ch.wait_senders.len() != before_senders;
        removed |= ch.wait_receivers.len() != before_receivers;
    }
    if removed {
        scheduler::wake_fiber(waiter);
    }
}
