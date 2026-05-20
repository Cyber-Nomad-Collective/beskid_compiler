//! Shared lazy initialization for runtime handle tables.

use std::sync::{Mutex, MutexGuard};

pub(crate) type LazySlotMap<T> = Mutex<Option<slotmap::SlotMap<slotmap::DefaultKey, T>>>;

pub(crate) fn lock_lazy_slot_map<T>(
    table: &'static LazySlotMap<T>,
    lock_name: &'static str,
) -> MutexGuard<'static, Option<slotmap::SlotMap<slotmap::DefaultKey, T>>> {
    let mut guard = table.lock().expect(lock_name);
    if guard.is_none() {
        *guard = Some(slotmap::SlotMap::with_key());
    }
    guard
}
