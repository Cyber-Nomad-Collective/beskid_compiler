//! Real foreign caller: links the Beskid-produced shared library, never native.rs.
use std::{ffi::c_void, mem::{align_of, offset_of, size_of}, ptr};
#[repr(C)] struct OwnedView { pointer: *const u8, length: usize, token: u64 }
impl OwnedView { fn zero() -> Self { Self { pointer: ptr::null(), length: 0, token: 0 } } }
unsafe extern "C" {
    fn glue_fixture_initialize() -> *mut c_void;
    fn glue_fixture_shutdown();
    fn beskid_i8(value: i8) -> i8;
    fn beskid_i16(value: i16) -> i16;
    fn beskid_i32(value: i32) -> i32;
    fn beskid_i64(value: i64) -> i64;
    fn beskid_u8(value: u8) -> u8;
    fn beskid_u16(value: u16) -> u16;
    fn beskid_u32(value: u32) -> u32;
    fn beskid_u64(value: u64) -> u64;
    fn beskid_f32(value: f32) -> f32;
    fn beskid_f64(value: f64) -> f64;
    fn beskid_bool(value: u8) -> u8;
    fn beskid_char(value: u32) -> u32;
    fn beskid_utf8(input: *const u8, length: usize, output: *mut OwnedView) -> i32;
    fn beskid_bytes(input: *const u8, length: usize, output: *mut OwnedView) -> i32;
    fn beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(token: u64) -> i32;
    fn beskid_handle(token: u64) -> u64;
    fn beskid_native_width(value: usize) -> usize;
    fn beskid_unit();
}
unsafe fn managed(function: unsafe extern "C" fn(*const u8, usize, *mut OwnedView) -> i32, bytes: &[u8]) {
    let mut out = OwnedView::zero();
    assert_eq!(unsafe { function(bytes.as_ptr(), bytes.len(), &mut out) }, 0);
    assert_ne!(out.token, 0);
    assert_eq!(out.length, bytes.len());
    assert!(!out.pointer.is_null() || out.length == 0);
    if out.length != 0 { assert_eq!(unsafe { std::slice::from_raw_parts(out.pointer, out.length) }, bytes); }
    assert_eq!(unsafe { beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(out.token) }, 0);
    assert_eq!(unsafe { beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(out.token) }, 1);
    // Never read the released pointer.
}
fn main() {
    assert_eq!((size_of::<OwnedView>(), align_of::<OwnedView>()), (24, 8));
    assert_eq!((offset_of!(OwnedView, pointer), offset_of!(OwnedView, length), offset_of!(OwnedView, token)), (0, 8, 16));
    unsafe {
        assert!(!glue_fixture_initialize().is_null(), "canonical runtime host initializes before exports");
        macro_rules! integers { ($f:ident, $t:ty) => { for v in [<$t>::MIN, 0, 1, <$t>::MAX] { assert_eq!($f(v), v); } }; }
        integers!(beskid_i8, i8);
        integers!(beskid_i16, i16);
        integers!(beskid_i32, i32);
        integers!(beskid_i64, i64);
        integers!(beskid_u8, u8);
        integers!(beskid_u16, u16);
        integers!(beskid_u32, u32);
        integers!(beskid_u64, u64);
        for b in [0_u32, 0x80000000, 1, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc12345] { assert_eq!(beskid_f32(f32::from_bits(b)).to_bits(), b); }
        for b in [0_u64, 0x8000000000000000, 1, 0x7fefffffffffffff, 0x7ff0000000000000, 0xfff0000000000000, 0x7ff8123456789abc] { assert_eq!(beskid_f64(f64::from_bits(b)).to_bits(), b); }
        assert_eq!(beskid_bool(0), 0); assert_eq!(beskid_bool(1), 1);
        for scalar in [0, 65, 0x1f600, 0x10ffff] { assert_eq!(beskid_char(scalar), scalar); }
        managed(beskid_utf8, b"A\0\xf0\x9f\x98\x80"); managed(beskid_utf8, b"");
        managed(beskid_bytes, &[0,128,255,65]); managed(beskid_bytes, &[]);
        for (function, input, length) in [
            (beskid_utf8 as unsafe extern "C" fn(*const u8, usize, *mut OwnedView) -> i32, [255].as_ptr(), 1),
            (beskid_bytes, ptr::null(), 1),
            (beskid_bytes, [0].as_ptr(), 16*1024*1024+1),
        ] {
            let mut out = OwnedView { pointer: 1_usize as *const u8, length: 99, token: 99 };
            assert_eq!(function(input, length, &mut out), 1);
            assert!(out.pointer.is_null()); assert_eq!((out.length, out.token), (0,0));
        }
        assert_eq!(beskid_utf8(ptr::null(), 0, ptr::null_mut()), 1);
        let mut storage = [0_u64; 4];
        assert_eq!(beskid_bytes(ptr::null(), 0, storage.as_mut_ptr().cast::<u8>().add(1).cast()), 1);
        assert_eq!(beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(0), 1);
        assert_eq!(beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(u64::MAX), 1);
        assert_eq!(beskid_handle(0x8000000000000001), 0x8000000000000001);
        assert_eq!(beskid_native_width(usize::MAX), usize::MAX); beskid_unit();
        let mut before_restart = OwnedView::zero();
        assert_eq!(beskid_bytes([0_u8,128,255,65].as_ptr(), 4, &mut before_restart), 0);
        let prior_token = before_restart.token;
        assert_ne!(prior_token, 0);
        glue_fixture_shutdown();
        // State storage is reused; the process-lifetime issuer must not reset.
        assert!(!glue_fixture_initialize().is_null());
        let mut after_restart = OwnedView::zero();
        assert_eq!(beskid_bytes([0_u8,128,255,65].as_ptr(), 4, &mut after_restart), 0);
        assert_ne!(after_restart.token, prior_token);
        assert_eq!(beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(prior_token), 1);
        assert_eq!(beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(after_restart.token), 0);
        glue_fixture_shutdown();
    }
    println!("manual Beskid export calls: 17 representations plus checked managed failures");
}
