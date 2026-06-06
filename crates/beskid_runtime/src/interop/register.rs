//! Handler registration for manifest-aligned soft dispatch tags.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use beskid_abi::BESKID_RUNTIME_ABI_VERSION;

use crate::generated::dispatch_table::{
    DISPATCH_GROUP_I64, DISPATCH_GROUP_PTR, DISPATCH_GROUP_UNIT, DISPATCH_GROUP_USIZE,
};

type UsizeHandler = unsafe extern "C" fn(*const u8) -> usize;
type PtrHandler = unsafe extern "C" fn(*const u8) -> *mut u8;
type UnitHandler = unsafe extern "C" fn(*const u8);
type I64Handler = unsafe extern "C" fn(*const u8) -> i64;

/// One slot in the host-provided handler registration table.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HandlerTableEntry {
    pub group: u32,
    pub tag: u32,
    pub fn_ptr: *const u8,
}

unsafe impl Send for HandlerTableEntry {}
unsafe impl Sync for HandlerTableEntry {}

struct HandlerOverrides {
    usize_handlers: HashMap<u32, UsizeHandler>,
    ptr_handlers: HashMap<u32, PtrHandler>,
    unit_handlers: HashMap<u32, UnitHandler>,
    i64_handlers: HashMap<u32, I64Handler>,
}

impl HandlerOverrides {
    fn new() -> Self {
        Self {
            usize_handlers: HashMap::new(),
            ptr_handlers: HashMap::new(),
            unit_handlers: HashMap::new(),
            i64_handlers: HashMap::new(),
        }
    }
}

static HANDLER_OVERRIDES: OnceLock<Mutex<HandlerOverrides>> = OnceLock::new();

fn handler_overrides() -> &'static Mutex<HandlerOverrides> {
    HANDLER_OVERRIDES.get_or_init(|| Mutex::new(HandlerOverrides::new()))
}

/// Register handler overrides for soft dispatch tags.
///
/// Returns `0` on success, `1` when `version` does not match [`BESKID_RUNTIME_ABI_VERSION`],
/// and `2` when `table` is null or `count` is zero.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_register_handlers(
    version: u64,
    table: *const HandlerTableEntry,
    count: u64,
) -> i32 {
    if version != u64::from(BESKID_RUNTIME_ABI_VERSION) {
        return 1;
    }
    if table.is_null() {
        return 2;
    }
    if count == 0 {
        *handler_overrides().lock().expect("handler overrides") = HandlerOverrides::new();
        return 0;
    }

    let entries = unsafe { std::slice::from_raw_parts(table, count as usize) };
    let mut overrides = HandlerOverrides::new();
    for entry in entries {
        let handler = entry.fn_ptr;
        if handler.is_null() {
            continue;
        }
        match entry.group {
            DISPATCH_GROUP_USIZE => {
                overrides
                    .usize_handlers
                    .insert(entry.tag, unsafe { std::mem::transmute(handler) });
            }
            DISPATCH_GROUP_PTR => {
                overrides
                    .ptr_handlers
                    .insert(entry.tag, unsafe { std::mem::transmute(handler) });
            }
            DISPATCH_GROUP_UNIT => {
                overrides
                    .unit_handlers
                    .insert(entry.tag, unsafe { std::mem::transmute(handler) });
            }
            DISPATCH_GROUP_I64 => {
                overrides
                    .i64_handlers
                    .insert(entry.tag, unsafe { std::mem::transmute(handler) });
            }
            _ => {}
        }
    }

    *handler_overrides().lock().expect("handler overrides") = overrides;
    0
}

/// Attempt a registered `usize` handler override for `tag`.
pub unsafe fn try_dispatch_usize(tag: i32, enum_ptr: *const u8) -> Option<usize> {
    let table = handler_overrides().lock().ok()?;
    let handler = table.usize_handlers.get(&(tag as u32))?;
    Some(unsafe { handler(enum_ptr) })
}

/// Attempt a registered `ptr` handler override for `tag`.
pub unsafe fn try_dispatch_ptr(tag: i32, enum_ptr: *const u8) -> Option<*mut u8> {
    let table = handler_overrides().lock().ok()?;
    let handler = table.ptr_handlers.get(&(tag as u32))?;
    Some(unsafe { handler(enum_ptr) })
}

/// Attempt a registered `unit` handler override for `tag`.
pub unsafe fn try_dispatch_unit(tag: i32, enum_ptr: *const u8) -> bool {
    let Ok(table) = handler_overrides().lock() else {
        return false;
    };
    let Some(handler) = table.unit_handlers.get(&(tag as u32)) else {
        return false;
    };
    unsafe {
        handler(enum_ptr);
    }
    true
}

/// Attempt a registered `i64` handler override for `tag`.
pub unsafe fn try_dispatch_i64(tag: i32, enum_ptr: *const u8) -> Option<i64> {
    let table = handler_overrides().lock().ok()?;
    let handler = table.i64_handlers.get(&(tag as u32))?;
    Some(unsafe { handler(enum_ptr) })
}

use std::sync::Once;

static HANDLER_BOOTSTRAP: Once = Once::new();

/// Idempotent process bootstrap: accept kernel-only dispatch until host registers overrides.
pub fn bootstrap_dispatch_handlers() {
    HANDLER_BOOTSTRAP.call_once(|| {
        let _ =
            beskid_register_handlers(u64::from(BESKID_RUNTIME_ABI_VERSION), std::ptr::null(), 0);
    });
}

/// Trap when a host-owned dispatch tag is invoked without [`beskid_register_handlers`].
#[cold]
pub fn trap_missing_host_handler(tag: i32) -> ! {
    panic!(
        "host dispatch handler not registered (tag {tag}). \
         Link `beskid_host` and call `beskid_host_register_all`, \
         or use `--runtime-profile std`."
    );
}
