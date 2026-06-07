//! Host interop: decode enum tags from generated payloads and dispatch to Rust handlers.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::interop_layout::enum_tag;

pub mod dispatch_table;
pub mod register;

/// Unit-returning interop dispatch; panics when the tag is unknown or the handler returns false.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn interop_dispatch_unit(enum_ptr: *const u8) {
    let tag = enum_tag(enum_ptr);
    if unsafe { dispatch_table::dispatch_unit(tag, enum_ptr) } {
        return;
    }
    panic!("invalid interop tag for unit dispatch");
}

/// Scalar-returning interop dispatch for the `usize` return group only.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn interop_dispatch_usize(enum_ptr: *const u8) -> usize {
    let tag = enum_tag(enum_ptr);
    if let Some(value) = unsafe { dispatch_table::dispatch_usize(tag, enum_ptr) } {
        return value;
    }
    panic!("invalid interop tag for usize dispatch");
}

/// Scalar-returning interop dispatch for the `i64` return group only.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn interop_dispatch_i64(enum_ptr: *const u8) -> i64 {
    let tag = enum_tag(enum_ptr);
    if let Some(value) = unsafe { dispatch_table::dispatch_i64(tag, enum_ptr) } {
        return value;
    }
    panic!("invalid interop tag for i64 dispatch");
}

/// Pointer-returning interop dispatch; panics on unknown tag or missing handler.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn interop_dispatch_ptr(enum_ptr: *const u8) -> *mut u8 {
    let tag = enum_tag(enum_ptr);
    if let Some(value) = unsafe { dispatch_table::dispatch_ptr(tag, enum_ptr) } {
        return value;
    }
    panic!("invalid interop tag for ptr dispatch");
}
