//! Host callback registration (`beskid_register_callbacks`) and GC-safe trampolines.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use beskid_abi::BESKID_USER_FFI_LAYOUT_BAND;

use crate::gc::{enter_runtime_scope, leave_runtime_scope};

/// One slot in the host-provided callback registration table.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CallbackTableEntry {
    pub symbol_id: u32,
    pub fn_ptr: *const u8,
    pub userdata: *mut c_void,
}

unsafe impl Send for CallbackTableEntry {}
unsafe impl Sync for CallbackTableEntry {}

static CALLBACK_TABLE: OnceLock<Mutex<Vec<CallbackTableEntry>>> = OnceLock::new();

#[derive(Clone, Copy)]
struct TrampolineTarget(*const u8);

unsafe impl Send for TrampolineTarget {}
unsafe impl Sync for TrampolineTarget {}

static TRAMPOLINE_TARGETS: OnceLock<Mutex<HashMap<u32, TrampolineTarget>>> = OnceLock::new();

fn callback_table() -> &'static Mutex<Vec<CallbackTableEntry>> {
    CALLBACK_TABLE.get_or_init(|| Mutex::new(Vec::new()))
}

fn trampoline_targets() -> &'static Mutex<HashMap<u32, TrampolineTarget>> {
    TRAMPOLINE_TARGETS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register host callbacks for the current process.
///
/// Returns `0` on success, `1` when `version` does not match [`BESKID_USER_FFI_LAYOUT_BAND`],
/// and `2` when `table` is null or `count` is zero.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_register_callbacks(
    version: u32,
    table: *const CallbackTableEntry,
    count: usize,
) -> i32 {
    if version != BESKID_USER_FFI_LAYOUT_BAND {
        return 1;
    }
    if table.is_null() || count == 0 {
        return 2;
    }
    let entries = unsafe { std::slice::from_raw_parts(table, count) }.to_vec();
    *callback_table().lock().expect("callback table") = entries;
    0
}

/// Returns a stable trampoline pointer for a Beskid export that enters runtime scope before
/// executing Beskid code. Re-entrant safe: each trampoline call acquires a nested runtime scope.
///
/// `symbol_id` is stored alongside `beskid_fn_ptr` so the shared trampoline can resolve the
/// concrete target from [`CallbackTableEntry`] rows registered via [`beskid_register_callbacks`].
pub fn install_callback_trampoline(beskid_fn_ptr: *const u8, symbol_id: u32) -> *const u8 {
    trampoline_targets()
        .lock()
        .expect("trampoline targets")
        .insert(symbol_id, TrampolineTarget(beskid_fn_ptr));
    trampoline_for_i64_fn as *const u8
}

extern "C-unwind" fn trampoline_for_i64_fn() -> i64 {
    enter_runtime_scope();
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            leave_runtime_scope();
        }
    }
    let _guard = Guard;

    let trampoline_ptr = trampoline_for_i64_fn as *const u8;
    let table = callback_table().lock().expect("callback table");
    let entry = table
        .iter()
        .find(|entry| entry.fn_ptr == trampoline_ptr)
        .or_else(|| table.first());
    let Some(entry) = entry else {
        return 0;
    };

    let target = trampoline_targets()
        .lock()
        .expect("trampoline targets")
        .get(&entry.symbol_id)
        .map(|target| target.0)
        .unwrap_or(entry.fn_ptr);

    if target.is_null() {
        return 0;
    }

    let callable: extern "C" fn() -> i64 = unsafe { std::mem::transmute(target) };
    callable()
}

/// Snapshot of the registered callback table (test / diagnostics).
pub fn registered_callbacks() -> Vec<CallbackTableEntry> {
    callback_table().lock().expect("callback table").clone()
}
