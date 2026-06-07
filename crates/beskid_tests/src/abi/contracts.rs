use std::collections::HashSet;
use std::path::PathBuf;

use crate::support::runtime::with_runtime_scope;
use beskid_abi::{
    BESKID_RUNTIME_ABI_VERSION, BUILTIN_SPECS, BeskidArray, BeskidStr, DispatchReturnGroup,
    RUNTIME_EXPORT_SYMBOLS, SYM_ABI_VERSION, SYM_ALLOC, SYM_BESKID_REGISTER_CALLBACKS,
    SYM_BESKID_REGISTER_HANDLERS, SYM_COMPOSITION_BIND_PLURAL, SYM_COMPOSITION_CONTAINER_CREATE,
    SYM_COMPOSITION_CONTAINER_DROP, SYM_COMPOSITION_LAUNCH, SYM_COMPOSITION_REGISTER,
    SYM_COMPOSITION_RESOLVE, SYM_COMPOSITION_RESOLVE_PLURAL, SYM_COMPOSITION_SCOPE_DEPTH,
    SYM_COMPOSITION_SCOPE_ENTER, SYM_COMPOSITION_SCOPE_LEAVE, SYM_COMPOSITION_SHUTDOWN,
    SYM_DYNAMIC_CAST_CHECKED, SYM_DYNAMIC_CELL_CREATE, SYM_DYNAMIC_CELL_WRAP, SYM_DYNAMIC_MAP_AOT,
    SYM_DYNAMIC_MAP_FALLBACK, SYM_DYNAMIC_OBJECT_ALLOC, SYM_FIBER_YIELD, SYM_GC_REGISTER_ROOT,
    SYM_GC_ROOT_HANDLE, SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER,
    SYM_INTEROP_DISPATCH_PTR, SYM_INTEROP_DISPATCH_UNIT, SYM_INTEROP_DISPATCH_USIZE,
    SYM_INTEROP_DISPATCH_I64, SYM_PANIC,
    SYM_PANIC_STR, SYM_RUNTIME_PREEMPT_CHECK, SYM_STR_LEN, SYM_SYSCALL_READ, SYM_SYSCALL_WRITE,
    TAG_FIBER_PROCESSOR_COUNT, TAG_FIBER_SPAWN_WITH_CANCEL_SLOT, TAG_FS_WRITE_TEXT,
    dispatch_route_for_symbol, is_dispatch_symbol,
};
use beskid_aot::runtime::{RuntimeBuildRequest, prepare_runtime};
use beskid_aot::{AotError, RuntimeStrategy};
use beskid_pipeline::phases::{
    MACRO_EXPAND, MOD_ANALYZE, MOD_COLLECT, MOD_GENERATE, MOD_LOAD, MOD_REWRITE, SEMANTIC_SNAPSHOT,
    SYNTAX_GENERATION,
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
fn abi_version_is_v4() {
    assert_eq!(BESKID_RUNTIME_ABI_VERSION, 4);
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
fn fiber_dispatch_tags_cover_processor_count_and_cancel_slot_spawn() {
    let processor = dispatch_route_for_symbol("fiber_processor_count")
        .expect("fiber_processor_count should be a dispatch symbol");
    assert_eq!(processor.tag, TAG_FIBER_PROCESSOR_COUNT);
    assert_eq!(processor.group, DispatchReturnGroup::I64);

    let cancel_slot = dispatch_route_for_symbol("fiber_spawn_with_cancel_slot")
        .expect("fiber_spawn_with_cancel_slot should be a dispatch symbol");
    assert_eq!(cancel_slot.tag, TAG_FIBER_SPAWN_WITH_CANCEL_SLOT);
    assert_eq!(cancel_slot.group, DispatchReturnGroup::I64);

    assert!(
        !RUNTIME_EXPORT_SYMBOLS.contains(&"fiber_processor_count"),
        "soft fiber ops must not be kernel exports in ABI v4"
    );
}

#[test]
fn runtime_export_symbols_match_frozen_kernel_allowlist_v4() {
    let expected = vec![
        SYM_ABI_VERSION,
        SYM_ALLOC,
        SYM_BESKID_REGISTER_CALLBACKS,
        SYM_BESKID_REGISTER_HANDLERS,
        SYM_COMPOSITION_BIND_PLURAL,
        SYM_COMPOSITION_CONTAINER_CREATE,
        SYM_COMPOSITION_CONTAINER_DROP,
        SYM_COMPOSITION_LAUNCH,
        SYM_COMPOSITION_REGISTER,
        SYM_COMPOSITION_RESOLVE,
        SYM_COMPOSITION_RESOLVE_PLURAL,
        SYM_COMPOSITION_SCOPE_DEPTH,
        SYM_COMPOSITION_SCOPE_ENTER,
        SYM_COMPOSITION_SCOPE_LEAVE,
        SYM_COMPOSITION_SHUTDOWN,
        SYM_DYNAMIC_CAST_CHECKED,
        SYM_DYNAMIC_CELL_CREATE,
        SYM_DYNAMIC_CELL_WRAP,
        SYM_DYNAMIC_MAP_AOT,
        SYM_DYNAMIC_MAP_FALLBACK,
        SYM_DYNAMIC_OBJECT_ALLOC,
        SYM_FIBER_YIELD,
        SYM_GC_REGISTER_ROOT,
        SYM_GC_ROOT_HANDLE,
        SYM_GC_UNREGISTER_ROOT,
        SYM_GC_UNROOT_HANDLE,
        SYM_GC_WRITE_BARRIER,
        SYM_INTEROP_DISPATCH_PTR,
        SYM_INTEROP_DISPATCH_UNIT,
        SYM_INTEROP_DISPATCH_USIZE,
        SYM_INTEROP_DISPATCH_I64,
        SYM_PANIC,
        SYM_PANIC_STR,
        SYM_RUNTIME_PREEMPT_CHECK,
    ];
    let mut expected_sorted = expected;
    let mut actual: Vec<_> = RUNTIME_EXPORT_SYMBOLS.iter().copied().collect();
    expected_sorted.sort();
    actual.sort();
    assert_eq!(actual, expected_sorted);
}

#[test]
fn runtime_export_symbols_are_unique() {
    let set: HashSet<&'static str> = RUNTIME_EXPORT_SYMBOLS.iter().copied().collect();
    assert_eq!(set.len(), RUNTIME_EXPORT_SYMBOLS.len());
}

#[test]
fn mvp_corelib_ops_route_through_dispatch_or_kernel() {
    assert!(
        is_dispatch_symbol(SYM_STR_LEN),
        "`str_len` should be a dispatch symbol in ABI v4"
    );
    assert!(
        is_dispatch_symbol(SYM_SYSCALL_WRITE),
        "`syscall_write` should be a dispatch symbol in ABI v4"
    );
    assert!(
        is_dispatch_symbol(SYM_SYSCALL_READ),
        "`syscall_read` should be a dispatch symbol in ABI v4"
    );
    assert!(
        RUNTIME_EXPORT_SYMBOLS.contains(&SYM_INTEROP_DISPATCH_USIZE),
        "kernel must export usize dispatch entrypoint"
    );
    assert!(
        RUNTIME_EXPORT_SYMBOLS.contains(&SYM_INTEROP_DISPATCH_PTR),
        "kernel must export ptr dispatch entrypoint"
    );
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
fn host_ops_are_not_kernel_exports_v4() {
    for symbol in [
        "fs_write_text",
        "fs_read_text",
        "env_get",
        "env_set",
        "process_getpid",
        "tty_winsize",
    ] {
        assert!(
            !RUNTIME_EXPORT_SYMBOLS.contains(&symbol),
            "host op `{symbol}` must not be a kernel export in ABI v4"
        );
    }
}

#[test]
fn host_manifest_entries_match_generated_registration_table() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(crate_dir.join("../../runtime_manifest.bsol"))
        .expect("runtime manifest should be readable");
    let host_handlers =
        std::fs::read_to_string(crate_dir.join("../beskid_host/src/generated/host_handlers.rs"))
            .expect("generated host handler table should be readable");

    let manifest_host_count = manifest.matches("owner = host").count();
    let registration_count = host_handlers.matches("HandlerTableEntry {").count();

    assert_eq!(
        registration_count, manifest_host_count,
        "every host-owned dispatch tag must have a generated registration entry"
    );
}

#[test]
#[should_panic(expected = "host dispatch handler not registered")]
fn minimal_profile_host_tag_traps_without_registration() {
    let _ = beskid_runtime::beskid_register_handlers(
        u64::from(BESKID_RUNTIME_ABI_VERSION),
        std::ptr::null(),
        0,
    );
    let mut envelope = [0u8; 16];
    envelope[8..12].copy_from_slice(&(TAG_FS_WRITE_TEXT as i32).to_le_bytes());

    let _ = beskid_runtime::interop_dispatch_usize(envelope.as_ptr());
}

#[test]
fn runtime_array_len_matches_array_new_length() {
    with_runtime_scope(|_, _| {
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
