//! Hand-written Rust library side of the v0.6.1 Glue extension C ABI (`extensions.h`), built as
//! a cdylib. It establishes the record, discriminant and callback behaviour that the generated
//! Rust owner shell must reproduce.
use std::sync::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtPoint {
    pub tag: u8,
    pub x: i64,
    pub weight: f64,
    pub visible: u8,
    pub mark: u32,
}

pub type ExtVisit = unsafe extern "C" fn(u64, i64, *mut i64) -> i32;

static RETAINED: Mutex<Option<(ExtVisit, u64)>> = Mutex::new(None);

/// Record by value in, record through the out parameter. Out-of-domain Bool or char fields are
/// rejected before any effect.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_point(value: ExtPoint, out: *mut ExtPoint) -> i32 {
    if out.is_null() || value.visible > 1 || char::from_u32(value.mark).is_none() {
        return 1;
    }
    let result = ExtPoint {
        tag: value.tag.wrapping_add(1),
        x: value.x.wrapping_mul(2),
        weight: value.weight + 0.5,
        visible: 1 - value.visible,
        mark: value.mark,
    };
    unsafe { out.write(result) };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_next_color(value: u32, out: *mut u32) -> i32 {
    if out.is_null() || value >= 3 {
        return 1;
    }
    unsafe { out.write((value + 1) % 3) };
    0
}

/// Returns a discriminant that names no variant; the receiving side must reject it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_bad_color(out: *mut u32) -> i32 {
    if out.is_null() {
        return 1;
    }
    unsafe { out.write(7) };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_sum_each(count: i64, visit: ExtVisit, context: u64, out: *mut i64) -> i32 {
    if out.is_null() {
        return 1;
    }
    let mut sum = 0i64;
    for index in 0..count {
        let mut value = 0;
        let status = unsafe { visit(context, index, &mut value) };
        if status != 0 {
            return status;
        }
        sum = sum.wrapping_add(value);
    }
    unsafe { out.write(sum) };
    0
}

/// Retains the callback beyond the call, which the protocol forbids; a later invocation must
/// fail with status 2 and run no consumer code.
#[unsafe(no_mangle)]
pub extern "C" fn ext_manual_retain(visit: ExtVisit, context: u64) -> i32 {
    *RETAINED.lock().unwrap() = Some((visit, context));
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_invoke_retained(index: i64, out: *mut i64) -> i32 {
    let Some((visit, context)) = *RETAINED.lock().unwrap() else { return 1 };
    unsafe { visit(context, index, out) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ext_manual_point_layout(values: *mut u64) -> i32 {
    if values.is_null() {
        return 1;
    }
    let layout = [
        size_of::<ExtPoint>(),
        align_of::<ExtPoint>(),
        core::mem::offset_of!(ExtPoint, tag),
        core::mem::offset_of!(ExtPoint, x),
        core::mem::offset_of!(ExtPoint, weight),
        core::mem::offset_of!(ExtPoint, visible),
        core::mem::offset_of!(ExtPoint, mark),
    ];
    for (index, value) in layout.into_iter().enumerate() {
        unsafe { values.add(index).write(value as u64) };
    }
    0
}
