use std::collections::HashSet;
use std::path::PathBuf;

use beskid_abi::{
    BESKID_RUNTIME_ABI_VERSION, BUILTIN_SPECS, BeskidArray, BeskidStr, RUNTIME_EXPORT_SYMBOLS,
    SYM_ABI_VERSION, SYM_ALLOC, SYM_ARRAY_LEN, SYM_ARRAY_NEW, SYM_CHANNEL_CLOSE, SYM_CHANNEL_CREATE,
    SYM_CHANNEL_RECEIVE, SYM_CHANNEL_RECEIVE_PTR, SYM_CHANNEL_RECEIVE_VALUE, SYM_CHANNEL_SEND,
    SYM_CHANNEL_SEND_PTR, SYM_CHANNEL_TRY_RECEIVE, SYM_CHANNEL_TRY_RECEIVE_PTR,
    SYM_CHANNEL_TRY_SEND, SYM_CHANNEL_TRY_SEND_PTR, SYM_COMPOSITION_BIND_PLURAL,
    SYM_COMPOSITION_CONTAINER_CREATE, SYM_COMPOSITION_CONTAINER_DROP, SYM_COMPOSITION_LAUNCH,
    SYM_COMPOSITION_REGISTER, SYM_COMPOSITION_RESOLVE, SYM_COMPOSITION_RESOLVE_PLURAL,
    SYM_COMPOSITION_SCOPE_DEPTH, SYM_COMPOSITION_SCOPE_ENTER, SYM_COMPOSITION_SCOPE_LEAVE,
    SYM_COMPOSITION_SHUTDOWN, SYM_EVENT_GET_HANDLER, SYM_EVENT_LEN, SYM_EVENT_SUBSCRIBE,
    SYM_EVENT_UNSUBSCRIBE_FIRST, SYM_GC_BYTES_ALLOCATED, SYM_GC_COLLECT, SYM_GC_COLLECT_IF_NEEDED,
    SYM_GC_EXTERNAL_ROOT_COUNT, SYM_GC_OBJECT_COUNT, SYM_GC_PHASE, SYM_GC_REGISTER_ROOT,
    SYM_GC_ROOT_HANDLE, SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER,
    SYM_HUB_CREATE, SYM_HUB_REGISTER, SYM_HUB_UNREGISTER, SYM_HUB_WAIT_RECEIVE,
    SYM_HUB_WAIT_RECEIVE_INDEX, SYM_HUB_WAIT_RECEIVE_VALUE, SYM_INTEROP_DISPATCH_PTR,
    SYM_INTEROP_DISPATCH_UNIT, SYM_INTEROP_DISPATCH_USIZE, SYM_MUTEX_CREATE, SYM_MUTEX_LOCK,
    SYM_MUTEX_TRY_LOCK, SYM_MUTEX_UNLOCK, SYM_PANIC, SYM_PANIC_STR, SYM_RUNTIME_PREEMPT_CHECK,
    SYM_STR_CONCAT, SYM_STR_EQ, SYM_STR_FROM_I64, SYM_STR_LEN, SYM_STR_NEW, SYM_SYSCALL_READ,
    SYM_SYSCALL_WRITE,
    SYM_TEST_BYTES_LEN, SYM_TEST_BYTES_PTR, SYM_WAIT_GROUP_ADD, SYM_WAIT_GROUP_CREATE,
    SYM_WAIT_GROUP_DONE, SYM_WAIT_GROUP_WAIT,
};
use beskid_aot::runtime::{RuntimeBuildRequest, prepare_runtime};
use beskid_aot::{AotError, RuntimeStrategy};
use beskid_engine::Engine;
use beskid_pipeline::phases::{
    MACRO_EXPAND, MOD_ANALYZE, MOD_COLLECT, MOD_GENERATE, MOD_LOAD, MOD_REWRITE,
    SEMANTIC_SNAPSHOT, SYNTAX_GENERATION,
};
use beskid_runtime::{array_len, array_new};

fn corelib_concurrency_source(module: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/concurrency/src/Concurrency")
        .join(format!("{module}.bd"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read corelib module `{}`: {err}", path.display()))
}

fn corelib_i64_constant(source: &str, name: &str) -> i64 {
    let signature = format!("pub i64 {name}()");
    let start = source
        .find(&signature)
        .unwrap_or_else(|| panic!("missing corelib i64 constant `{name}`"));
    let body = &source[start..];
    let return_start = body
        .find("return ")
        .unwrap_or_else(|| panic!("missing return for corelib i64 constant `{name}`"))
        + "return ".len();
    let return_body = &body[return_start..];
    let return_end = return_body
        .find(';')
        .unwrap_or_else(|| panic!("missing semicolon for corelib i64 constant `{name}`"));
    return_body[..return_end]
        .trim()
        .parse::<i64>()
        .unwrap_or_else(|err| panic!("invalid value for corelib i64 constant `{name}`: {err}"))
}

#[test]
fn builtin_symbols_are_unique() {
    let set: HashSet<&'static str> = BUILTIN_SPECS.iter().map(|spec| spec.symbol).collect();
    assert_eq!(set.len(), BUILTIN_SPECS.len());
}

#[test]
fn concurrency_corelib_status_codes_match_runtime_contract() {
    let status = corelib_concurrency_source("Status");
    let fiber_join_status = corelib_concurrency_source("FiberJoinStatus");

    assert_eq!(
        corelib_i64_constant(&status, "Ok"),
        beskid_runtime::status::STATUS_OK
    );
    assert_eq!(
        corelib_i64_constant(&status, "Closed"),
        beskid_runtime::status::STATUS_CLOSED
    );
    assert_eq!(
        corelib_i64_constant(&status, "Cancelled"),
        beskid_runtime::status::STATUS_CANCELLED
    );
    assert_eq!(
        corelib_i64_constant(&status, "WouldBlock"),
        beskid_runtime::status::STATUS_WOULD_BLOCK
    );
    assert_eq!(
        corelib_i64_constant(&status, "HubEmpty"),
        beskid_runtime::status::STATUS_HUB_EMPTY
    );
    assert_eq!(
        corelib_i64_constant(&status, "HubLimit"),
        beskid_runtime::status::STATUS_HUB_LIMIT
    );
    assert_eq!(
        corelib_i64_constant(&status, "HubNotFound"),
        beskid_runtime::status::STATUS_HUB_NOT_FOUND
    );
    assert_eq!(
        corelib_i64_constant(&status, "MutexBusy"),
        beskid_runtime::status::MUTEX_WOULD_BLOCK
    );

    assert_eq!(
        corelib_i64_constant(&fiber_join_status, "Ok"),
        beskid_runtime::status::FIBER_JOIN_OK
    );
    assert_eq!(
        corelib_i64_constant(&fiber_join_status, "Cancelled"),
        beskid_runtime::status::FIBER_JOIN_CANCELLED
    );
    assert_eq!(
        corelib_i64_constant(&fiber_join_status, "Panicked"),
        beskid_runtime::status::FIBER_JOIN_PANICKED
    );
    assert_eq!(
        corelib_i64_constant(&fiber_join_status, "StackOverflow"),
        beskid_runtime::status::FIBER_JOIN_STACK_OVERFLOW
    );
    assert_eq!(
        corelib_i64_constant(&fiber_join_status, "NotDone"),
        beskid_runtime::status::FIBER_JOIN_NOT_DONE
    );
}

#[test]
fn fiber_abi_symbols_cover_processor_count_and_cancel_slot_spawn() {
    let builtin_symbols: HashSet<&'static str> =
        BUILTIN_SPECS.iter().map(|spec| spec.symbol).collect();

    for symbol in ["fiber_processor_count", "fiber_spawn_with_cancel_slot"] {
        assert!(
            builtin_symbols.contains(symbol),
            "BUILTIN_SPECS should include `{symbol}`"
        );
        assert!(
            RUNTIME_EXPORT_SYMBOLS.contains(&symbol),
            "RUNTIME_EXPORT_SYMBOLS should include `{symbol}`"
        );
    }
}

#[test]
fn runtime_export_symbols_match_frozen_allowlist_snapshot() {
    let expected = vec![
        SYM_ABI_VERSION,
        SYM_ALLOC,
        SYM_STR_NEW,
        SYM_STR_CONCAT,
        SYM_STR_EQ,
        SYM_STR_FROM_I64,
        SYM_STR_LEN,
        SYM_ARRAY_NEW,
        SYM_ARRAY_LEN,
        SYM_PANIC,
        SYM_PANIC_STR,
        SYM_SYSCALL_WRITE,
        SYM_SYSCALL_READ,
        SYM_GC_BYTES_ALLOCATED,
        SYM_GC_OBJECT_COUNT,
        SYM_GC_PHASE,
        SYM_GC_COLLECT,
        SYM_GC_COLLECT_IF_NEEDED,
        SYM_GC_EXTERNAL_ROOT_COUNT,
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
        SYM_TEST_BYTES_PTR,
        SYM_TEST_BYTES_LEN,
        beskid_abi::SYM_FIBER_SPAWN,
        beskid_abi::SYM_FIBER_SPAWN_WITH_CANCEL_SLOT,
        beskid_abi::SYM_FIBER_JOIN,
        beskid_abi::SYM_FIBER_JOIN_VALUE,
        beskid_abi::SYM_FIBER_DETACH,
        beskid_abi::SYM_FIBER_CANCEL,
        beskid_abi::SYM_FIBER_YIELD,
        beskid_abi::SYM_FIBER_NOW_MILLIS,
        beskid_abi::SYM_FIBER_CURRENT_ID,
        beskid_abi::SYM_FIBER_PROCESSOR_COUNT,
        SYM_CHANNEL_CREATE,
        SYM_CHANNEL_SEND,
        SYM_CHANNEL_RECEIVE,
        SYM_CHANNEL_RECEIVE_VALUE,
        SYM_CHANNEL_TRY_SEND,
        SYM_CHANNEL_TRY_RECEIVE,
        SYM_CHANNEL_CLOSE,
        SYM_CHANNEL_SEND_PTR,
        SYM_CHANNEL_TRY_SEND_PTR,
        SYM_CHANNEL_RECEIVE_PTR,
        SYM_CHANNEL_TRY_RECEIVE_PTR,
        SYM_RUNTIME_PREEMPT_CHECK,
        SYM_HUB_CREATE,
        SYM_HUB_REGISTER,
        SYM_HUB_UNREGISTER,
        SYM_HUB_WAIT_RECEIVE,
        SYM_HUB_WAIT_RECEIVE_INDEX,
        SYM_HUB_WAIT_RECEIVE_VALUE,
        SYM_MUTEX_CREATE,
        SYM_MUTEX_LOCK,
        SYM_MUTEX_TRY_LOCK,
        SYM_MUTEX_UNLOCK,
        SYM_WAIT_GROUP_CREATE,
        SYM_WAIT_GROUP_ADD,
        SYM_WAIT_GROUP_DONE,
        SYM_WAIT_GROUP_WAIT,
        SYM_COMPOSITION_CONTAINER_CREATE,
        SYM_COMPOSITION_CONTAINER_DROP,
        SYM_COMPOSITION_REGISTER,
        SYM_COMPOSITION_BIND_PLURAL,
        SYM_COMPOSITION_LAUNCH,
        SYM_COMPOSITION_SHUTDOWN,
        SYM_COMPOSITION_SCOPE_ENTER,
        SYM_COMPOSITION_SCOPE_LEAVE,
        SYM_COMPOSITION_RESOLVE,
        SYM_COMPOSITION_RESOLVE_PLURAL,
        SYM_COMPOSITION_SCOPE_DEPTH,
        beskid_abi::SYM_BESKID_REGISTER_CALLBACKS,
        beskid_abi::SYM_DYNAMIC_CAST_CHECKED,
        beskid_abi::SYM_DYNAMIC_CELL_CREATE,
        beskid_abi::SYM_DYNAMIC_CELL_WRAP,
        beskid_abi::SYM_DYNAMIC_MAP_AOT,
        beskid_abi::SYM_DYNAMIC_MAP_FALLBACK,
        beskid_abi::SYM_DYNAMIC_OBJECT_ALLOC,
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
    engine.with_runtime(|_, _| {
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

#[test]
fn macro_expand_phase_id_matches_platform_contract() {
    assert_eq!(MACRO_EXPAND, "macro.expand");
}

#[test]
fn mod_pipeline_phase_ids_match_platform_contract() {
    let expected = [
        "mod.load",
        "mod.collect",
        "mod.generate",
        "syntax.generation",
        "semantic.snapshot",
        "mod.analyze",
        "mod.rewrite",
    ];
    assert_eq!(
        [
            MOD_LOAD,
            MOD_COLLECT,
            MOD_GENERATE,
            SYNTAX_GENERATION,
            SEMANTIC_SNAPSHOT,
            MOD_ANALYZE,
            MOD_REWRITE,
        ],
        expected
    );
}
