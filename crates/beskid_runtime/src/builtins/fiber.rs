use crate::builtins::events::EventState;
use crate::scheduler;

pub type FiberEntry = extern "C" fn(*mut u8) -> i64;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_spawn(entry: FiberEntry, env: *mut u8) -> i64 {
    scheduler::fiber_spawn(entry, env, std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_join(fiber_id: i64, out_value: *mut i64) -> i64 {
    scheduler::fiber_join(fiber_id, out_value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_join_status(fiber_id: i64) -> i64 {
    scheduler::fiber_join(fiber_id, std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_join_value(fiber_id: i64) -> i64 {
    let mut out = 0i64;
    let _ = scheduler::fiber_join(fiber_id, &mut out);
    out
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_detach(fiber_id: i64) {
    scheduler::fiber_detach(fiber_id);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_cancel(fiber_id: i64) {
    scheduler::fiber_cancel(fiber_id);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_yield() {
    scheduler::fiber_yield();
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_now_millis() -> i64 {
    scheduler::fiber_now_millis()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_current_id() -> i64 {
    scheduler::current_fiber_id()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_processor_count() -> i64 {
    scheduler::processor_count() as i64
}

/// Spawn with an **OnCancelled** event slot (used when lowering knows the handle layout).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fiber_spawn_with_cancel_slot(
    entry: FiberEntry,
    env: *mut u8,
    on_cancelled_slot: *mut *mut EventState,
) -> i64 {
    scheduler::fiber_spawn(entry, env, on_cancelled_slot)
}
