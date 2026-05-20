use std::collections::HashSet;
use std::path::PathBuf;

use beskid_abi::{
    BeskidArray, BeskidStr, BESKID_RUNTIME_ABI_VERSION, BUILTIN_SPECS, RUNTIME_EXPORT_SYMBOLS,
    SYM_ABI_VERSION, SYM_ALLOC, SYM_ARRAY_LEN, SYM_ARRAY_NEW, SYM_EVENT_GET_HANDLER, SYM_EVENT_LEN,
    SYM_EVENT_SUBSCRIBE, SYM_EVENT_UNSUBSCRIBE_FIRST, SYM_GC_REGISTER_ROOT, SYM_GC_ROOT_HANDLE,
    SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER, SYM_INTEROP_DISPATCH_PTR,
    SYM_INTEROP_DISPATCH_UNIT, SYM_INTEROP_DISPATCH_USIZE, SYM_PANIC, SYM_PANIC_STR,
    SYM_STR_CONCAT, SYM_STR_LEN, SYM_STR_NEW, SYM_SYSCALL_READ, SYM_SYSCALL_WRITE,
};
use beskid_aot::runtime::{prepare_runtime, RuntimeBuildRequest};
use beskid_aot::{AotError, RuntimeStrategy};
use beskid_engine::Engine;
use beskid_runtime::{array_len, array_new};

#[test]
fn builtin_symbols_are_unique() {
    let set: HashSet<&'static str> = BUILTIN_SPECS.iter().map(|spec| spec.symbol).collect();
    assert_eq!(set.len(), BUILTIN_SPECS.len());
}

#[test]
fn runtime_export_symbols_match_frozen_allowlist_snapshot() {
    let expected = vec![
        SYM_ABI_VERSION,
        SYM_ALLOC,
        SYM_STR_NEW,
        SYM_STR_CONCAT,
        SYM_STR_LEN,
        SYM_ARRAY_NEW,
        SYM_ARRAY_LEN,
        SYM_PANIC,
        SYM_PANIC_STR,
        SYM_SYSCALL_WRITE,
        SYM_SYSCALL_READ,
        SYM_GC_WRITE_BARRIER,
        SYM_GC_ROOT_HANDLE,
        SYM_GC_UNROOT_HANDLE,
        SYM_GC_REGISTER_ROOT,
        SYM_GC_UNREGISTER_ROOT,
        SYM_EVENT_SUBSCRIBE,
        SYM_EVENT_UNSUBSCRIBE_FIRST,
        SYM_EVENT_LEN,
        SYM_EVENT_GET_HANDLER,
        SYM_INTEROP_DISPATCH_UNIT,
        SYM_INTEROP_DISPATCH_PTR,
        SYM_INTEROP_DISPATCH_USIZE,
        beskid_abi::SYM_TEST_BYTES_PTR,
        beskid_abi::SYM_TEST_BYTES_LEN,
        beskid_abi::SYM_FIBER_SPAWN,
        beskid_abi::SYM_FIBER_JOIN,
        beskid_abi::SYM_FIBER_JOIN_VALUE,
        beskid_abi::SYM_FIBER_DETACH,
        beskid_abi::SYM_FIBER_CANCEL,
        beskid_abi::SYM_FIBER_YIELD,
        beskid_abi::SYM_FIBER_NOW_MILLIS,
        beskid_abi::SYM_FIBER_CURRENT_ID,
        beskid_abi::SYM_CHANNEL_CREATE,
        beskid_abi::SYM_CHANNEL_SEND,
        beskid_abi::SYM_CHANNEL_RECEIVE,
        beskid_abi::SYM_CHANNEL_RECEIVE_VALUE,
        beskid_abi::SYM_CHANNEL_TRY_SEND,
        beskid_abi::SYM_CHANNEL_TRY_RECEIVE,
        beskid_abi::SYM_CHANNEL_CLOSE,
        beskid_abi::SYM_HUB_CREATE,
        beskid_abi::SYM_HUB_REGISTER,
        beskid_abi::SYM_HUB_UNREGISTER,
        beskid_abi::SYM_HUB_WAIT_RECEIVE,
        beskid_abi::SYM_HUB_WAIT_RECEIVE_INDEX,
        beskid_abi::SYM_HUB_WAIT_RECEIVE_VALUE,
        beskid_abi::SYM_MUTEX_CREATE,
        beskid_abi::SYM_MUTEX_LOCK,
        beskid_abi::SYM_MUTEX_TRY_LOCK,
        beskid_abi::SYM_MUTEX_UNLOCK,
        beskid_abi::SYM_WAIT_GROUP_CREATE,
        beskid_abi::SYM_WAIT_GROUP_ADD,
        beskid_abi::SYM_WAIT_GROUP_DONE,
        beskid_abi::SYM_WAIT_GROUP_WAIT,
    ];
    assert_eq!(RUNTIME_EXPORT_SYMBOLS, expected);
}

#[test]
fn runtime_export_symbols_are_unique() {
    let set: HashSet<&'static str> = RUNTIME_EXPORT_SYMBOLS.iter().copied().collect();
    assert_eq!(set.len(), RUNTIME_EXPORT_SYMBOLS.len());
}

#[test]
fn runtime_exports_cover_mvp_corelib_symbols() {
    let required = [SYM_STR_LEN, SYM_SYSCALL_WRITE, SYM_SYSCALL_READ];
    for symbol in required {
        assert!(
            RUNTIME_EXPORT_SYMBOLS.contains(&symbol),
            "runtime exports should include MVP corelib symbol `{symbol}`"
        );
    }
}

#[test]
fn prebuilt_runtime_rejects_wrong_abi_version() {
    let path = PathBuf::from("/tmp/nonexistent-runtime-archive.a");
    let request = RuntimeBuildRequest {
        strategy: RuntimeStrategy::UsePrebuilt {
            path,
            abi_version: BESKID_RUNTIME_ABI_VERSION + 1,
        },
    };

    let err = prepare_runtime(&request).expect_err("expected ABI mismatch failure");
    assert!(matches!(
        err,
        AotError::RuntimeAbiMismatch {
            expected,
            actual
        } if expected == BESKID_RUNTIME_ABI_VERSION && actual == BESKID_RUNTIME_ABI_VERSION + 1
    ));
}

#[test]
fn prebuilt_runtime_missing_archive_fails() {
    let path = PathBuf::from("/tmp/missing-beskid-runtime-archive.a");
    let request = RuntimeBuildRequest {
        strategy: RuntimeStrategy::UsePrebuilt {
            path: path.clone(),
            abi_version: BESKID_RUNTIME_ABI_VERSION,
        },
    };

    let err = prepare_runtime(&request).expect_err("expected missing archive failure");
    assert!(matches!(err, AotError::RuntimeArchiveMissing { path: missing } if missing == path));
}

#[test]
fn runtime_array_len_matches_array_new_length() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let ptr = array_new(8, 3);
        assert!(!ptr.is_null(), "array_new should return a non-null handle");
        assert_eq!(
            array_len(ptr),
            3,
            "array_len(array_new(8, 3)) should report logical length 3"
        );
    });
}

#[test]
fn ffi_types_have_stable_sizes() {
    assert_eq!(
        std::mem::size_of::<BeskidStr>(),
        std::mem::size_of::<usize>() * 2
    );
    assert_eq!(
        std::mem::size_of::<BeskidArray>(),
        std::mem::size_of::<usize>() * 3
    );
}
