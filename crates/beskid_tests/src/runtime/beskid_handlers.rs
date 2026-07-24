use crate::support::runtime::with_runtime_scope;
use beskid_abi::{BeskidArray, BeskidStr, TAG_BYTES_COMPARE, TAG_BYTES_GET, TAG_STR_EQ, TAG_TEST_BYTES_LEN};
use beskid_runtime::builtins::bytes_compare as rust_bytes_compare;
use beskid_runtime::builtins::bytes_get as rust_bytes_get;
use beskid_runtime::builtins::test_bytes_len as rust_test_bytes_len;
use beskid_runtime::interop::dispatch_table::dispatch_i64;
use beskid_runtime::{HandlerTableEntry, beskid_register_handlers, str_eq as rust_str_eq};
use beskid_runtime_handlers::beskid_language_register_all;

#[repr(C)]
struct RuntimeInteropEnvelope {
    type_desc_ptr: *const u8,
    tag: i32,
    pad: i32,
}

#[repr(C)]
struct BytesCompareEnvelope {
    header: RuntimeInteropEnvelope,
    left: *const BeskidArray,
    right: *const BeskidArray,
}

#[repr(C)]
struct StrEqEnvelope {
    header: RuntimeInteropEnvelope,
    left: *const BeskidStr,
    right: *const BeskidStr,
}

#[repr(C)]
struct BytesGetEnvelope {
    header: RuntimeInteropEnvelope,
    array: *const BeskidArray,
    index: u64,
}

fn reset_handler_overrides() {
    const EMPTY: [HandlerTableEntry; 0] = [];
    assert_eq!(beskid_register_handlers(u64::from(beskid_abi::BESKID_RUNTIME_ABI_VERSION), EMPTY.as_ptr(), 0,), 0);
}

fn setup_language_handlers() {
    reset_handler_overrides();
    assert_eq!(beskid_language_register_all(), 0);
}

fn make_bytes(left: &[u8], right: &[u8]) -> (*const BeskidArray, *const BeskidArray) {
    let left_arr = beskid_runtime::array_new(1, left.len()) as *const BeskidArray;
    let right_arr = beskid_runtime::array_new(1, right.len()) as *const BeskidArray;
    unsafe {
        if !left.is_empty() {
            std::ptr::copy_nonoverlapping(left.as_ptr(), (*left_arr).ptr, left.len());
        }
        if !right.is_empty() {
            std::ptr::copy_nonoverlapping(right.as_ptr(), (*right_arr).ptr, right.len());
        }
    }
    (left_arr, right_arr)
}

#[test]
fn language_handlers_register_and_override_bytes_compare() {
    with_runtime_scope(|_, _| {
        setup_language_handlers();

        let (left, right) = make_bytes(b"abc", b"abd");
        let envelope = BytesCompareEnvelope {
            header: RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: TAG_BYTES_COMPARE, pad: 0 },
            left,
            right,
        };
        let enum_ptr = &envelope as *const BytesCompareEnvelope as *const u8;
        let handler_result = unsafe { dispatch_i64(TAG_BYTES_COMPARE, enum_ptr) };
        let rust_result = rust_bytes_compare(left, right);
        assert_eq!(handler_result, Some(rust_result));
        assert_eq!(handler_result, Some(-1));
    });
}

#[test]
fn language_handlers_str_eq_matches_rust_fallback() {
    with_runtime_scope(|_, _| {
        setup_language_handlers();

        let hello = b"hello";
        let left = beskid_runtime::str_new(hello.as_ptr(), hello.len());
        let right = beskid_runtime::str_new(hello.as_ptr(), hello.len());
        let envelope = StrEqEnvelope {
            header: RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: TAG_STR_EQ, pad: 0 },
            left,
            right,
        };
        let enum_ptr = &envelope as *const StrEqEnvelope as *const u8;
        let handler_result = unsafe { dispatch_i64(TAG_STR_EQ, enum_ptr) };
        let rust_result = rust_str_eq(left, right) as i64;
        assert_eq!(handler_result, Some(rust_result));
        assert_eq!(handler_result, Some(1));
    });
}

#[test]
fn language_handler_test_bytes_len_differs_from_rust_fallback() {
    with_runtime_scope(|_, _| {
        setup_language_handlers();

        let envelope = RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: TAG_TEST_BYTES_LEN, pad: 0 };
        let enum_ptr = &envelope as *const RuntimeInteropEnvelope as *const u8;
        let handler_result = unsafe { dispatch_i64(TAG_TEST_BYTES_LEN, enum_ptr) };
        let rust_result = rust_test_bytes_len() as i64;
        assert_eq!(handler_result, Some(beskid_runtime_handlers::test_bytes_len()));
        assert_ne!(handler_result, Some(rust_result));
    });
}

#[test]
fn language_handlers_bytes_get_matches_rust_fallback() {
    with_runtime_scope(|_, _| {
        setup_language_handlers();

        let data = b"abc";
        let (array, _) = make_bytes(data, b"");
        let envelope = BytesGetEnvelope {
            header: RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: TAG_BYTES_GET, pad: 0 },
            array,
            index: 1,
        };
        let enum_ptr = &envelope as *const BytesGetEnvelope as *const u8;
        let handler_result = unsafe { dispatch_i64(TAG_BYTES_GET, enum_ptr) };
        let rust_result = rust_bytes_get(array, 1);
        assert_eq!(handler_result, Some(rust_result));
        assert_eq!(handler_result, Some(b'b' as i64));
    });
}

/// GC and syscall dispatch ops remain on Rust fallbacks until cohort C3–C5
/// prerequisites land (OS access, scheduler phase A, kernel `gc_*` substrate).
#[test]
fn gc_and_syscall_ops_not_language_handlers_yet() {
    use beskid_analysis::runtime_registration::RUNTIME_HANDLER_SPECS;
    let deferred =
        ["syscall_write", "syscall_read", "gc_collect", "gc_bytes_allocated", "fiber_spawn", "channel_create"];
    for key in deferred {
        assert!(
            !RUNTIME_HANDLER_SPECS.iter().any(|spec| spec.dispatch_key == key),
            "{key} must not be registered as language_handler yet"
        );
    }
}

#[test]
fn empty_handler_table_clears_overrides() {
    setup_language_handlers();
    reset_handler_overrides();
}
