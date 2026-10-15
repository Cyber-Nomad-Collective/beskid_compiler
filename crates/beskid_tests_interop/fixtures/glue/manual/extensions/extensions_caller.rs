//! Hand-written consumer side of the v0.6.1 Glue extension C ABI. It plays the role the
//! generated consumer adapters play: it checks discriminants and Bool/char fields it receives,
//! and it links one callback frame per call on the calling thread, so a retained token or another
//! thread gets status 2.
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ExtPoint {
    tag: u8,
    x: i64,
    weight: f64,
    visible: u8,
    mark: u32,
}

type ExtVisit = unsafe extern "C" fn(u64, i64, *mut i64) -> i32;

#[link(name = "glue_extensions_manual")]
unsafe extern "C" {
    fn ext_manual_point(value: ExtPoint, out: *mut ExtPoint) -> i32;
    fn ext_manual_next_color(value: u32, out: *mut u32) -> i32;
    fn ext_manual_bad_color(out: *mut u32) -> i32;
    fn ext_manual_sum_each(count: i64, visit: ExtVisit, context: u64, out: *mut i64) -> i32;
    fn ext_manual_retain(visit: ExtVisit, context: u64) -> i32;
    fn ext_manual_invoke_retained(index: i64, out: *mut i64) -> i32;
    fn ext_manual_point_layout(values: *mut u64) -> i32;
}

thread_local! {
    /// Live callback frames of this thread: (token, scale).
    static FRAMES: RefCell<Vec<(u64, i64)>> = const { RefCell::new(Vec::new()) };
}
static TOKENS: AtomicU64 = AtomicU64::new(0);

/// Runs `body` with one callback frame linked; the frame is unlinked when `body` returns.
fn with_frame<T>(scale: i64, body: impl FnOnce(u64) -> T) -> T {
    let token = TOKENS.fetch_add(1, Ordering::Relaxed) + 1;
    FRAMES.with(|frames| frames.borrow_mut().push((token, scale)));
    let result = body(token);
    FRAMES.with(|frames| frames.borrow_mut().retain(|(live, _)| *live != token));
    result
}

unsafe extern "C" fn visit(context: u64, index: i64, out: *mut i64) -> i32 {
    if out.is_null() {
        return 1;
    }
    let Some(scale) = FRAMES.with(|frames| frames.borrow().iter().find(|(token, _)| *token == context).map(|(_, scale)| *scale))
    else {
        return 2;
    };
    unsafe { out.write(index * scale) };
    0
}

/// The consumer-side check of a received discriminant.
fn checked_color(value: u32) -> Result<u32, i32> {
    if value < 3 { Ok(value) } else { Err(1) }
}

fn main() {
    let mut layout = [0u64; 7];
    assert_eq!(unsafe { ext_manual_point_layout(layout.as_mut_ptr()) }, 0);
    let expected = [
        size_of::<ExtPoint>() as u64,
        align_of::<ExtPoint>() as u64,
        core::mem::offset_of!(ExtPoint, tag) as u64,
        core::mem::offset_of!(ExtPoint, x) as u64,
        core::mem::offset_of!(ExtPoint, weight) as u64,
        core::mem::offset_of!(ExtPoint, visible) as u64,
        core::mem::offset_of!(ExtPoint, mark) as u64,
    ];
    assert_eq!(layout, expected, "library and consumer record layouts differ");
    assert_eq!(layout, [32, 8, 0, 8, 16, 24, 28], "record layout differs from the C ABI profile");

    let input = ExtPoint { tag: 255, x: -21, weight: 1.25, visible: 1, mark: 0x1f600 };
    let mut output = ExtPoint { tag: 0, x: 0, weight: 0.0, visible: 0, mark: 0 };
    assert_eq!(unsafe { ext_manual_point(input, &mut output) }, 0);
    assert_eq!(output, ExtPoint { tag: 0, x: -42, weight: 1.75, visible: 0, mark: 0x1f600 });
    for invalid in [ExtPoint { visible: 2, ..input }, ExtPoint { mark: 0xd800, ..input }] {
        assert_eq!(unsafe { ext_manual_point(invalid, &mut output) }, 1, "{invalid:?}");
    }

    for (value, next) in [(0, 1), (1, 2), (2, 0)] {
        let mut out = u32::MAX;
        assert_eq!(unsafe { ext_manual_next_color(value, &mut out) }, 0);
        assert_eq!(checked_color(out), Ok(next));
    }
    let mut out = 0;
    assert_eq!(unsafe { ext_manual_next_color(3, &mut out) }, 1);
    assert_eq!(unsafe { ext_manual_bad_color(&mut out) }, 0);
    assert_eq!(checked_color(out), Err(1), "a discriminant that names no variant must be rejected");

    let sum = with_frame(3, |token| {
        let mut sum = 0;
        assert_eq!(unsafe { ext_manual_sum_each(4, visit, token, &mut sum) }, 0);
        sum
    });
    assert_eq!(sum, 3 * 6);

    // A callback the foreign side retains is dead once the call that passed it returned.
    with_frame(5, |token| assert_eq!(unsafe { ext_manual_retain(visit, token) }, 0));
    let mut value = -1;
    assert_eq!(unsafe { ext_manual_invoke_retained(1, &mut value) }, 2);
    assert_eq!(value, -1, "a stale callback wrote its result");

    // Another thread finds no frame for a live token.
    with_frame(7, |token| {
        let status = std::thread::spawn(move || {
            let mut value = 0;
            unsafe { visit(token, 1, &mut value) }
        })
        .join()
        .unwrap();
        assert_eq!(status, 2);
    });
    println!("glue extension manual C ABI: ok");
}
