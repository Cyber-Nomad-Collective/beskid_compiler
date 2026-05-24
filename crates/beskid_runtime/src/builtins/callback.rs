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

#[derive(Clone, Copy)]
struct TrampolinePtr(*const u8);

unsafe impl Send for TrampolinePtr {}
unsafe impl Sync for TrampolinePtr {}

static CALLBACK_TABLE: OnceLock<Mutex<Vec<CallbackTableEntry>>> = OnceLock::new();
static TRAMPOLINE_TABLE: OnceLock<Mutex<HashMap<usize, TrampolinePtr>>> = OnceLock::new();

fn callback_table() -> &'static Mutex<Vec<CallbackTableEntry>> {
    CALLBACK_TABLE.get_or_init(|| Mutex::new(Vec::new()))
}

fn trampoline_table() -> &'static Mutex<HashMap<usize, TrampolinePtr>> {
    TRAMPOLINE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
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
pub fn install_callback_trampoline(beskid_fn_ptr: *const u8) -> *const u8 {
    let key = beskid_fn_ptr as usize;
    let mut table = trampoline_table().lock().expect("trampoline table");
    if let Some(existing) = table.get(&key) {
        return existing.0;
    }
    let trampoline = TrampolinePtr(trampoline_for_i64_fn as *const u8);
    table.insert(key, trampoline);
    trampoline.0
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
    // Phase A: host invokes through a typed export; the concrete target is resolved by symbol_id
    // in future dispatch wiring. For registration tests the trampoline only establishes scope.
    0
}

/// Snapshot of the registered callback table (test / diagnostics).
pub fn registered_callbacks() -> Vec<CallbackTableEntry> {
    callback_table().lock().expect("callback table").clone()
}
