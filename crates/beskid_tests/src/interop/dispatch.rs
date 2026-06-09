use beskid_abi::BeskidStr;
use beskid_abi::{
    RUNTIME_EXPORT_SYMBOLS, SYM_INTEROP_DISPATCH_PTR, SYM_INTEROP_DISPATCH_UNIT,
    SYM_INTEROP_DISPATCH_USIZE, TAG_CHANNEL_CREATE, TAG_STR_LEN,
};
use beskid_runtime::interop::dispatch_table::{dispatch_ptr, dispatch_unit, dispatch_usize};
use beskid_runtime::interop_dispatch_i64;

#[repr(C)]
struct RuntimeInteropEnvelope {
    type_desc_ptr: *const u8,
    tag: i32,
    pad: i32,
    payload_ptr: *const BeskidStr,
}

#[test]
fn runtime_exports_include_all_interop_dispatch_symbols() {
    assert!(
        RUNTIME_EXPORT_SYMBOLS.contains(&SYM_INTEROP_DISPATCH_UNIT),
        "missing unit interop dispatch symbol export"
    );
    assert!(
        RUNTIME_EXPORT_SYMBOLS.contains(&SYM_INTEROP_DISPATCH_USIZE),
        "missing usize interop dispatch symbol export"
    );
    assert!(
        RUNTIME_EXPORT_SYMBOLS.contains(&SYM_INTEROP_DISPATCH_PTR),
        "missing ptr interop dispatch symbol export"
    );
}

#[test]
fn return_group_routing_uses_usize_dispatch_for_string_len_tag() {
    let hello = b"hello";
    let value = BeskidStr {
        ptr: hello.as_ptr(),
        len: hello.len(),
    };

    let envelope = RuntimeInteropEnvelope {
        type_desc_ptr: std::ptr::null(),
        tag: TAG_STR_LEN,
        pad: 0,
        payload_ptr: &value,
    };

    let enum_ptr = &envelope as *const RuntimeInteropEnvelope as *const u8;
    let usize_result = unsafe { dispatch_usize(TAG_STR_LEN, enum_ptr) };
    assert_eq!(usize_result, Some(5));
}

#[test]
fn colliding_tag_two_routes_by_return_group() {
    assert_eq!(TAG_STR_LEN, TAG_CHANNEL_CREATE);
    beskid_runtime::run_closure_as_main(|| {
        let mut envelope = [0u8; 32];
        envelope[8..12].copy_from_slice(&TAG_CHANNEL_CREATE.to_le_bytes());
        envelope[16..24].copy_from_slice(&0i64.to_le_bytes());
        envelope[24..32].copy_from_slice(&0i64.to_le_bytes());
        let enum_ptr = envelope.as_ptr();
        let channel_id = interop_dispatch_i64(enum_ptr);
        assert!(
            channel_id > 0,
            "i64 interop must route tag 2 to channel_create, not str_len"
        );
        0
    });
}

#[test]
fn unknown_tag_returns_fallback_for_all_return_groups() {
    let envelope = RuntimeInteropEnvelope {
        type_desc_ptr: std::ptr::null(),
        tag: 404,
        pad: 0,
        payload_ptr: std::ptr::null(),
    };

    let enum_ptr = &envelope as *const RuntimeInteropEnvelope as *const u8;
    assert!(!unsafe { dispatch_unit(404, enum_ptr) });
    assert_eq!(unsafe { dispatch_usize(404, enum_ptr) }, None);
    assert_eq!(unsafe { dispatch_ptr(404, enum_ptr) }, None);
}
