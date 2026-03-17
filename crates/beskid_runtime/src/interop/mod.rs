use crate::interop_layout::enum_tag;

pub mod dispatch_table;

#[unsafe(no_mangle)]
pub extern "C" fn interop_dispatch_unit(enum_ptr: *const u8) {
    let tag = enum_tag(enum_ptr);
    if unsafe { dispatch_table::dispatch_unit(tag, enum_ptr) } {
        return;
    }
    panic!("invalid interop tag for unit dispatch");
}

#[unsafe(no_mangle)]
pub extern "C" fn interop_dispatch_usize(enum_ptr: *const u8) -> usize {
    let tag = enum_tag(enum_ptr);
    if let Some(value) = unsafe { dispatch_table::dispatch_usize(tag, enum_ptr) } {
        return value;
    }
    panic!("invalid interop tag for usize dispatch");
}

#[unsafe(no_mangle)]
pub extern "C" fn interop_dispatch_ptr(enum_ptr: *const u8) -> *mut u8 {
    let tag = enum_tag(enum_ptr);
    if let Some(value) = unsafe { dispatch_table::dispatch_ptr(tag, enum_ptr) } {
        return value;
    }
    panic!("invalid interop tag for ptr dispatch");
}
