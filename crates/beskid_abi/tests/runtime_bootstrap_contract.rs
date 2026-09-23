use beskid_abi::abi_v5::{
    ABI_V5, AbiManifestV5, AbiType, AssemblySymbol, CANONICAL_RUNTIME_PACKAGE_NAME,
    CANONICAL_RUNTIME_PACKAGE_PUBLISHER, ManifestValidationError, RuntimeAuditMetadata, RuntimePackageIdentity,
    TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS, TargetMetadata, canonical_runtime_package, render_runtime_asm_include,
    render_runtime_c_header,
};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitValidationError,
};

fn supported_targets() -> Vec<TargetMetadata> {
    let targets = TargetMetadata::supported();
    ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"]
        .into_iter()
        .map(|triple| targets.iter().find(|target| target.triple.as_str() == triple).unwrap().clone())
        .collect()
}

#[test]
fn descriptor_worker_layout_preserves_abi_v5_offsets() {
    for target in supported_targets() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let worker = manifest.layouts.iter().find(|layout| layout.name == "BeskidWorkerRequest").unwrap();
        assert_eq!((worker.size, worker.alignment), (72, 8));
        let descriptor = worker.fields.iter().find(|field| field.offset == 24).unwrap();
        assert_eq!(descriptor.ty, "usize", "pointer-width storage remains a checked descriptor slot");
        for (name, offset) in [("buffer", 32), ("length", 40), ("result", 48), ("owner_scheduler_id", 64)] {
            assert_eq!(worker.fields.iter().find(|field| field.name == name).unwrap().offset, offset);
        }
    }
}

#[test]
fn windows_descriptor_primitives_use_application_ucrt_import_authority() {
    let target =
        supported_targets().into_iter().find(|target| target.triple.as_str() == "x86_64-pc-windows-msvc").unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target);
    for symbol in
        ["_errno", "_dup", "_close", "_read", "_write", "_setmode", "_set_thread_local_invalid_parameter_handler"]
    {
        let import = manifest
            .platform_imports
            .iter()
            .find(|import| import.symbol == symbol)
            .unwrap_or_else(|| panic!("missing application UCRT descriptor import: {symbol}"));
        assert_eq!(import.library, "ucrt", "{symbol} must share the application's dynamic UCRT");
        if symbol == "_errno" {
            assert!(import.params.is_empty(), "_errno has no parameters");
            assert_eq!(import.result, AbiType::Pointer, "_errno returns the thread-local errno address");
        }
    }
}

#[test]
fn raw_descriptor_abi_rejects_before_allocation_or_native_admission() {
    let source = include_str!("../../../runtime/beskid/src/Runtime/Io/Syscalls.bd");
    let submit = source.find("pub i64 SyscallSubmitAndPark").unwrap();
    let reject = source[submit..].find("if descriptor > 2147483647 { return -1; }").unwrap() + submit;
    let allocation = source[submit..].find("pointer nativeBuffer = SystemAllocate").unwrap() + submit;
    assert!(reject < allocation, "raw descriptor admission must precede native allocation");

    let write = source.find("pub i64 SyscallWrite(i64 descriptor, pointer value)").unwrap();
    let write_reject =
        source[write..].find("if descriptor < 0 || descriptor > 2147483647 { return -1; }").unwrap() + write;
    let native_admission = source[write..].find("SyscallSubmitAndPark(word(descriptor)").unwrap() + write;
    assert!(write_reject < native_admission, "the i64 raw ABI must not narrow/admit an invalid descriptor");
}

#[test]
fn descriptor_worker_ownership_is_disjoint_from_the_existing_abandoned_protocol() {
    let source = include_str!("../assembly/common/external_wait.h");
    assert!(source.contains("BESKID_WORKER_ABANDONED = 1u"));
    assert!(source.contains("BESKID_WORKER_OWNS_DESCRIPTOR = UINT32_C(0x80000000)"));
    assert!(source.contains("request->abandoned |= BESKID_WORKER_OWNS_DESCRIPTOR"));
    assert!(source.contains("r->abandoned |= BESKID_WORKER_ABANDONED"));
    assert!(!source.contains("r->abandoned = 1"));
    assert!(
        source.contains("BeskidWorkerCloseDescriptor(request);"),
        "every request free releases its owned duplicate"
    );
    assert!(
        source.contains("if (beskid_workers_stop || beskid_request_count >= BESKID_REQUEST_MAX) {\n    BeskidWorkerCloseDescriptor(r);"),
        "queue rejection must release its duplicate before returning"
    );
}

#[test]
fn canonical_contract_has_the_exact_lifecycle_closure_and_trap_exports() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    manifest.validate().expect("canonical runtime contract");

    let actual = manifest
        .exports
        .iter()
        .map(|entry| (entry.symbol.as_str(), entry.params.as_slice(), entry.result))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            ("beskid_library_attach_v5", &[AbiType::Pointer][..], AbiType::I32,),
            ("beskid_library_detach_v5", &[AbiType::Pointer][..], AbiType::Void,),
            ("beskid_rt_v5_abi_value_clear", &[AbiType::Pointer][..], AbiType::U8),
            (
                "beskid_rt_v5_abi_value_initialize",
                &[AbiType::Pointer, AbiType::USize, AbiType::Pointer, AbiType::Pointer][..],
                AbiType::U8
            ),
            ("beskid_rt_v5_abi_value_move_out", &[AbiType::Pointer, AbiType::Pointer][..], AbiType::U8),
            ("beskid_rt_v5_abi_value_replace_with_barrier", &[AbiType::Pointer, AbiType::Pointer][..], AbiType::U8),
            ("beskid_rt_v5_abi_version", &[][..], AbiType::U32),
            ("beskid_rt_v5_array_allocate_rooted", &[AbiType::Pointer, AbiType::Pointer][..], AbiType::Pointer,),
            ("beskid_rt_v5_array_construction_finish", &[AbiType::Pointer][..], AbiType::U8,),
            (
                "beskid_rt_v5_array_grow_rooted",
                &[AbiType::Pointer, AbiType::USize, AbiType::Pointer][..],
                AbiType::Pointer,
            ),
            ("beskid_rt_v5_array_write_barrier", &[AbiType::Pointer, AbiType::Pointer][..], AbiType::U8,),
            (
                "beskid_rt_v5_closure_capture_store",
                &[AbiType::Pointer, AbiType::Pointer, AbiType::USize, AbiType::Pointer,][..],
                AbiType::U8,
            ),
            ("beskid_rt_v5_closure_environment_allocate", &[AbiType::Pointer][..], AbiType::Pointer,),
            (
                "beskid_rt_v5_closure_environment_root",
                &[AbiType::Pointer, AbiType::USize, AbiType::Pointer][..],
                AbiType::U8,
            ),
            ("beskid_rt_v5_closure_environment_root_current", &[AbiType::USize, AbiType::Pointer][..], AbiType::U8,),
            ("beskid_rt_v5_external_active_count", &[][..], AbiType::USize),
            ("beskid_rt_v5_external_owner_id", &[][..], AbiType::USize),
            ("beskid_rt_v5_external_pump", &[AbiType::I64][..], AbiType::Void),
            ("beskid_rt_v5_external_sleep_until", &[AbiType::I64][..], AbiType::USize),
            ("beskid_rt_v5_external_try_complete", &[AbiType::USize, AbiType::USize, AbiType::USize][..], AbiType::U8),
            ("beskid_rt_v5_external_wait_park", &[AbiType::USize][..], AbiType::USize),
            ("beskid_rt_v5_external_wait_post", &[AbiType::USize, AbiType::USize, AbiType::USize][..], AbiType::U8),
            (
                "beskid_rt_v5_external_wait_register",
                &[AbiType::USize, AbiType::USize, AbiType::I64][..],
                AbiType::USize
            ),
            ("beskid_rt_v5_external_wait_release", &[AbiType::USize][..], AbiType::U8),
            ("beskid_rt_v5_fiber_yield", &[][..], AbiType::Void,),
            ("beskid_rt_v5_heap_set_cap", &[AbiType::USize][..], AbiType::U8,),
            ("beskid_rt_v5_managed_object_allocate", &[AbiType::Pointer][..], AbiType::Pointer,),
            ("beskid_rt_v5_poll_executor_run_once", &[][..], AbiType::I32,),
            (
                "beskid_rt_v5_poll_executor_spawn",
                &[AbiType::Pointer, AbiType::Pointer, AbiType::Pointer, AbiType::Pointer][..],
                AbiType::I64,
            ),
            ("beskid_rt_v5_poll_executor_wake", &[AbiType::I64][..], AbiType::I32,),
            ("beskid_rt_v5_poll_link_clone", &[AbiType::I64][..], AbiType::I64,),
            ("beskid_rt_v5_poll_link_drop", &[AbiType::I64][..], AbiType::Void,),
            ("beskid_rt_v5_poll_link_new", &[AbiType::I64][..], AbiType::I64,),
            ("beskid_rt_v5_poll_link_poll", &[AbiType::I64, AbiType::Pointer][..], AbiType::I32,),
            ("beskid_rt_v5_poll_monitor_drop", &[AbiType::I64][..], AbiType::Void,),
            ("beskid_rt_v5_poll_monitor_new", &[AbiType::I64][..], AbiType::I64,),
            ("beskid_rt_v5_poll_monitor_poll", &[AbiType::I64, AbiType::Pointer][..], AbiType::I32,),
            ("beskid_rt_v5_process_init", &[AbiType::Pointer][..], AbiType::Pointer,),
            ("beskid_rt_v5_process_shutdown", &[AbiType::Pointer][..], AbiType::Void,),
            ("beskid_rt_v5_scheduler_stack_check", &[AbiType::USize][..], AbiType::U8,),
            ("beskid_rt_v5_scheduler_stack_overflow_observed", &[][..], AbiType::Void,),
            ("beskid_rt_v5_thread_attach", &[AbiType::Pointer][..], AbiType::Pointer,),
            ("beskid_rt_v5_thread_detach", &[AbiType::Pointer][..], AbiType::Void,),
            ("beskid_rt_v5_trap", &[AbiType::U8, AbiType::Pointer, AbiType::USize][..], AbiType::Void,),
        ]
    );
    assert_eq!(TRAP_EXIT_STATUS, 101);
    assert_eq!(TRAP_DIAGNOSTIC_PREFIX, "beskid runtime trap v5");
    let trap = manifest.exports.iter().find(|entry| entry.symbol == "beskid_rt_v5_trap").unwrap();
    assert!(trap.noreturn);
}

#[test]
fn trusted_intrinsics_are_typed_and_owned_only_by_the_canonical_package() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let package = canonical_runtime_package();
    assert_eq!(package.publisher(), CANONICAL_RUNTIME_PACKAGE_PUBLISHER);
    assert_eq!(package.name(), CANONICAL_RUNTIME_PACKAGE_NAME);
    assert_eq!(package.abi_version(), ABI_V5);
    let names = manifest.trusted_runtime_intrinsics.iter().map(|intrinsic| intrinsic.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names.len(), 68);
    assert!(names.contains(&"pointer_add"));
    assert!(names.contains(&"raw_word_load"));
    assert!(names.contains(&"system_allocate"));
    assert!(names.contains(&"guarded_stack_allocate"));
    assert!(names.contains(&"guarded_stack_grow"));
    assert!(names.contains(&"guarded_stack_free"));
    assert!(names.contains(&"tls_get"));
    assert!(names.contains(&"trap"));
    assert!(names.contains(&"clock_monotonic_nanos"));
    assert!(names.contains(&"process_getpid"));
    assert!(names.contains(&"fiber_yield"));
    assert!(names.contains(&"env_get"));
    assert!(names.contains(&"fs_read_text"));
    assert!(names.contains(&"tty_winsize"));
    assert!(names.contains(&"worker_submit"));
    for name in
        ["worker_release", "owner_create", "owner_destroy", "owner_post", "owner_pop", "owner_wait", "wait_claim"]
    {
        assert!(names.contains(&name));
    }
    for name in [
        "network_open",
        "network_close",
        "network_bind",
        "network_address",
        "network_options",
        "network_get_options",
        "network_shutdown_write",
        "network_reactor_create",
        "network_reactor_destroy",
        "network_submit",
        "network_cancel",
        "network_reactor_poll",
        "network_report_leak",
        "network_table_lock",
        "network_table_unlock",
        "network_table_root",
        "network_dns_start",
        "network_dns_count",
        "network_dns_get",
        "network_dns_release",
    ] {
        assert!(names.contains(&name), "missing private networking transport intrinsic {name}");
    }
    assert!(manifest.intrinsic_metadata("pointer_add").is_some());

    let mut unauthorized = manifest.clone();
    unauthorized.trusted_runtime_package = Some(
        serde_json::from_str::<RuntimePackageIdentity>(
            r#"{"publisher":"beskid-lang.org","name":"user-runtime-lookalike","abi_version":5}"#,
        )
        .unwrap(),
    );
    assert!(matches!(unauthorized.validate(), Err(ManifestValidationError::UnauthorizedRuntimePackage { .. })));
}

#[test]
fn runtime_provenance_allows_intrinsics_without_making_them_loader_requirements() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let audit = RuntimeAuditMetadata::for_manifest(
        &manifest,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .expect("canonical audit metadata");

    assert!(audit.allowed_exports.contains(&"beskid_rt_v5_process_init".into()));
    assert!(audit.allowed_exports.contains(&"beskid_rt_v5_intrinsic_memory_compare".into()));
    assert!(audit.allowed_exports.contains(&"beskid_rt_v5_intrinsic_guarded_stack_allocate".into()));
    assert!(audit.allowed_exports.contains(&"beskid_rt_v5_intrinsic_guarded_stack_grow".into()));
    assert!(audit.loader_required_exports.contains(&"beskid_rt_v5_process_init".into()));
    assert!(!audit.loader_required_exports.contains(&"beskid_rt_v5_intrinsic_memory_compare".into()));
}

#[test]
fn canonical_layouts_freeze_common_and_target_context_offsets() {
    let expected_contexts = [
        ("BeskidArchContextX86_64SysV", 64, 16, "rip", 56),
        ("BeskidArchContextAarch64Darwin", 176, 16, "d15", 168),
        ("BeskidArchContextX86_64Windows", 240, 16, "xmm15", 224),
    ];
    for (target, expected) in supported_targets().into_iter().zip(expected_contexts) {
        let is_windows = target.triple.as_str() == "x86_64-pc-windows-msvc";
        let manifest = AbiManifestV5::canonical_runtime(target);
        manifest.validate().expect("canonical target layout");
        let names = manifest.layouts.iter().map(|layout| layout.name.as_str()).collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "BeskidAbiValue",
                "BeskidAllocationRequest",
                "BeskidArrayAllocationRequest",
                "BeskidArrayElementDescriptor",
                "BeskidCallbackEntry",
                "BeskidCallbackRegistry",
                "BeskidCompositionContainer",
                "BeskidCompositionScope",
                "BeskidFiberRecord",
                "BeskidGcHandleSlot",
                "BeskidHandle",
                "BeskidHeapRegion",
                "BeskidHeapState",
                "BeskidNetworkHandle",
                "BeskidNetworkRequest",
                "BeskidObjectHeader",
                "BeskidPendingSpawn",
                "BeskidPollLink",
                "BeskidPollMonitor",
                "BeskidPollState",
                "BeskidPollTask",
                "BeskidRootFrame",
                "BeskidRootSlot",
                "BeskidRuntimeState",
                "BeskidSchedulerState",
                "BeskidTlsState",
                "BeskidTypeDescriptor",
                "BeskidWorkerRequest",
                expected.0,
            ]
        );
        let context = manifest.layouts.iter().find(|layout| layout.name == expected.0).unwrap();
        assert_eq!((context.size, context.alignment), (expected.1, expected.2));
        assert_eq!(context.fields.iter().find(|field| field.name == expected.3).unwrap().offset, expected.4);
        if is_windows {
            assert_eq!(context.fields.iter().find(|field| field.name == "xmm6").unwrap().ty, "v128");
        }

        let object = manifest.layouts.iter().find(|layout| layout.name == "BeskidObjectHeader").unwrap();
        assert_eq!((object.size, object.alignment), (16, 8));
        assert_eq!(object.fields[0].name, "descriptor");
        assert_eq!(object.fields[0].offset, 0);
        assert_eq!(object.fields[1].name, "gc_word");
        assert_eq!(object.fields[1].offset, 8);

        let tls = manifest.layouts.iter().find(|layout| layout.name == "BeskidTlsState").unwrap();
        assert_eq!((tls.size, tls.alignment), (48, 8));
        assert_eq!(
            tls.fields.iter().map(|field| (field.name.as_str(), field.offset)).collect::<Vec<_>>(),
            vec![
                ("runtime", 0),
                ("root_frame", 8),
                ("reserved", 16),
                ("attach_depth", 24),
                ("composition_scope", 32),
                ("composition_depth", 40),
            ]
        );

        let runtime = manifest.layouts.iter().find(|layout| layout.name == "BeskidRuntimeState").unwrap();
        assert_eq!((runtime.size, runtime.alignment), (64, 8));
        assert_eq!(runtime.fields.iter().find(|field| field.name == "abi_version").unwrap().offset, 0);
        assert_eq!(runtime.fields.iter().find(|field| field.name == "current_thread").unwrap().offset, 8);
        assert_eq!(
            runtime.fields.iter().find(|field| field.name == "root_frame").unwrap().offset,
            40,
            "RuntimeState.root_frame must stay distinct from TlsState.root_frame@8"
        );
    }
}

#[test]
fn heap_state_and_heap_region_layouts_match_the_growable_heap_design() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());

    let heap_state = manifest.layouts.iter().find(|layout| layout.name == "BeskidHeapState").unwrap();
    assert_eq!((heap_state.size, heap_state.alignment), (328, 8));
    let expected_heap_state_fields: &[(&str, u64)] = &[
        ("first_region", 0),
        ("current_region", 8),
        ("region_count", 16),
        ("committed_bytes", 24),
        ("live_bytes", 32),
        ("live_count", 40),
        ("collection_count", 48),
        ("collection_threshold", 56),
        ("gc_phase", 64),
        ("cap_bytes", 72),
        ("next_region_size", 80),
        ("failure_reason", 88),
        ("failure_request_bytes", 96),
        ("diagnostic", 104),
        ("root_stack_head", 168),
        ("root_stack_current", 176),
        ("handle_stack_head", 184),
        ("handle_stack_current", 192),
        ("current_span_by_class", 200),
        ("mark_fifo_head", 312),
        ("mark_fifo_tail", 320),
    ];
    for (name, offset) in expected_heap_state_fields {
        assert_eq!(
            heap_state.fields.iter().find(|field| field.name == *name).unwrap().offset,
            *offset,
            "BeskidHeapState.{name} must stay at offset {offset}"
        );
    }
    assert_eq!(heap_state.fields.iter().find(|field| field.name == "diagnostic").unwrap().ty, "u8[64]");
    // Root/handle capacity is unbounded (2026-09-22, span-heap redesign): the fixed
    // `external_roots`/`handles` arrays (raised 63/8 -> 4095/512 as a stopgap, then found to be
    // the real SIGILL cause: a recursive-descent regex/PEG match holds far more than any fixed
    // ceiling of GC-managed locals live across its call stack at once) are replaced by segmented,
    // growable root and handle stacks (`BeskidRootStackBlock`, `BeskidHandleStackBlock`,
    // runtime-internal, not `runtime_manifest.bsol` layouts). See
    // docs/superpowers/specs/2026-09-22-gc-span-heap-design.md and
    // runtime/beskid/src/Runtime/Mem/Gc/{State,Span,RootsHandles}.bd.
    assert_eq!(heap_state.fields.iter().find(|field| field.name == "current_span_by_class").unwrap().ty, "pointer[14]");

    let heap_region = manifest.layouts.iter().find(|layout| layout.name == "BeskidHeapRegion").unwrap();
    assert_eq!((heap_region.size, heap_region.alignment), (64, 8));
    let expected_heap_region_fields: &[(&str, u64)] = &[
        ("next", 0),
        ("size", 8),
        ("bump", 16),
        ("limit", 24),
        ("free_bytes", 32),
        ("largest_free", 40),
        ("objects_start", 48),
        ("free_span_list", 56),
    ];
    for (name, offset) in expected_heap_region_fields {
        assert_eq!(
            heap_region.fields.iter().find(|field| field.name == *name).unwrap().offset,
            *offset,
            "BeskidHeapRegion.{name} must stay at offset {offset}"
        );
    }
}

#[test]
fn target_system_imports_are_exact_and_unknown_contracts_are_rejected() {
    let unix_imports = [
        "_exit",
        "atan2",
        "ceil",
        "clock_gettime",
        "close",
        "cos",
        "fabs",
        "fcntl",
        "floor",
        "fstat",
        "getcwd",
        "getenv",
        "getpid",
        "ioctl",
        "log",
        "log10",
        "log2",
        "memcpy",
        "memset",
        "mkdir",
        "mmap",
        "mprotect",
        "munmap",
        "open",
        "pow",
        "pthread_cond_broadcast",
        "pthread_cond_destroy",
        "pthread_cond_init",
        "pthread_cond_wait",
        "pthread_create",
        "pthread_join",
        "pthread_mutex_lock",
        "pthread_mutex_unlock",
        "read",
        "setenv",
        "sin",
        "sqrt",
        "stat",
        "strlen",
        "tan",
        "unlink",
        "write",
    ];
    let windows_imports = [
        "_close",
        "_dup",
        "_errno",
        "_read",
        "_set_thread_local_invalid_parameter_handler",
        "_setmode",
        "_write",
        "AcquireSRWLockExclusive",
        "CloseHandle",
        "CreateDirectoryW",
        "CreateFileW",
        "CreateThread",
        "DeleteFileW",
        "ExitProcess",
        "GetConsoleScreenBufferInfo",
        "GetCurrentDirectoryW",
        "GetCurrentProcessId",
        "GetEnvironmentVariableW",
        "GetFileAttributesW",
        "GetFileSizeEx",
        "GetLastError",
        "GetStdHandle",
        "GetSystemTimeAsFileTime",
        "GetTickCount64",
        "InitOnceExecuteOnce",
        "InitializeConditionVariable",
        "MultiByteToWideChar",
        "ReadFile",
        "ReleaseSRWLockExclusive",
        "SetEnvironmentVariableW",
        "SetLastError",
        "SleepConditionVariableSRW",
        "TlsAlloc",
        "TlsGetValue",
        "TlsSetValue",
        "VirtualAlloc",
        "VirtualFree",
        "WaitForSingleObject",
        "WakeAllConditionVariable",
        "WriteFile",
        "atan2",
        "ceil",
        "cos",
        "fabs",
        "floor",
        "log",
        "log10",
        "log2",
        "memset",
        "pow",
        "sin",
        "sqrt",
        "tan",
    ];
    let unix_network_imports = [
        "accept",
        "bind",
        "connect",
        "freeaddrinfo",
        "getaddrinfo",
        "getpeername",
        "getsockname",
        "getsockopt",
        "listen",
        "pipe",
        "recv",
        "recvmsg",
        "send",
        "sendto",
        "setsockopt",
        "shutdown",
        "socket",
    ];
    let windows_winsock_imports = [
        "WSAGetLastError",
        "WSAIoctl",
        "WSARecv",
        "WSARecvFrom",
        "WSASend",
        "WSASendTo",
        "WSASocketW",
        "WSAStartup",
        "bind",
        "closesocket",
        "connect",
        "freeaddrinfo",
        "getaddrinfo",
        "getpeername",
        "getsockname",
        "getsockopt",
        "listen",
        "setsockopt",
        "shutdown",
    ];
    let windows_completion_imports =
        ["CancelIoEx", "CreateIoCompletionPort", "GetQueuedCompletionStatus", "PostQueuedCompletionStatus"];
    let math_imports = ["atan2", "ceil", "cos", "fabs", "floor", "log", "log10", "log2", "pow", "sin", "sqrt", "tan"];
    let windows_ucrt_descriptor_imports =
        ["_close", "_dup", "_errno", "_read", "_set_thread_local_invalid_parameter_handler", "_setmode", "_write"];
    for target in supported_targets() {
        let is_windows = target.triple.as_str() == "x86_64-pc-windows-msvc";
        let (mut expected_symbols, expected_library) = match target.triple.as_str() {
            "aarch64-apple-darwin" => {
                let mut imports = unix_imports.to_vec();
                imports.extend(unix_network_imports);
                imports.extend(["bzero", "pthread_cond_timedwait_relative_np", "kevent", "kqueue"]);
                (imports, None)
            }
            "x86_64-unknown-linux-gnu" => {
                let mut imports = unix_imports.to_vec();
                imports.extend(unix_network_imports);
                imports.extend(["epoll_create1", "epoll_ctl", "epoll_wait"]);
                imports.extend([
                    "pthread_cond_timedwait",
                    "pthread_condattr_init",
                    "pthread_condattr_destroy",
                    "pthread_condattr_setclock",
                ]);
                (imports, Some(("libc", "libm")))
            }
            "x86_64-pc-windows-msvc" => {
                let mut imports = windows_imports.to_vec();
                imports.extend(windows_winsock_imports);
                imports.extend(windows_completion_imports);
                imports.push("memcpy");
                (imports, Some(("kernel32", "ucrt")))
            }
            unsupported => panic!("unsupported target in contract test: {unsupported}"),
        };
        expected_symbols.sort_unstable();
        let mut manifest = AbiManifestV5::canonical_runtime(target);
        assert_eq!(
            manifest.platform_imports.iter().map(|entry| entry.symbol.as_str()).collect::<Vec<_>>(),
            expected_symbols
        );
        match expected_library {
            None => assert!(manifest.platform_imports.iter().all(|entry| entry.library == "libSystem")),
            Some((platform, math)) => {
                for entry in &manifest.platform_imports {
                    let expected = if is_windows && matches!(entry.symbol.as_str(), "memcpy" | "memset") {
                        "vcruntime"
                    } else if is_windows && windows_winsock_imports.contains(&entry.symbol.as_str()) {
                        "ws2_32"
                    } else if math_imports.contains(&entry.symbol.as_str())
                        || (is_windows && windows_ucrt_descriptor_imports.contains(&entry.symbol.as_str()))
                    {
                        math
                    } else {
                        platform
                    };
                    assert_eq!(entry.library, expected, "incorrect provider for {}", entry.symbol);
                }
            }
        }
        manifest.platform_imports.pop();
        assert!(matches!(manifest.validate(), Err(ManifestValidationError::InvalidPlatformImportSet { .. })));
    }

    let mut duplicate = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    duplicate.layouts.push(duplicate.layouts[0].clone());
    assert!(matches!(duplicate.validate(), Err(ManifestValidationError::DuplicateLayout { .. })));

    let mut unknown = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    unknown.exports[0].symbol = "beskid_rt_v5_surprise".into();
    assert!(matches!(unknown.validate(), Err(ManifestValidationError::InvalidRuntimeExportSet { .. })));
}

#[test]
fn generated_headers_are_deterministic_fresh_and_include_contract_constants() {
    for target in supported_targets() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let c_header = render_runtime_c_header(&manifest).unwrap();
        let asm_include = render_runtime_asm_include(&manifest).unwrap();
        assert_eq!(c_header, render_runtime_c_header(&manifest).unwrap());
        assert_eq!(asm_include, render_runtime_asm_include(&manifest).unwrap());
        assert!(c_header.contains("#define BESKID_RUNTIME_ABI_VERSION 5"));
        assert!(c_header.contains("#define BESKID_TRAP_EXIT_STATUS 101"));
        assert!(c_header.lines().any(|line| line == "#define BESKID_OBJECT_HEADER_DESCRIPTOR_OFFSET 0"));
        assert!(c_header.contains("beskid_rt_v5_process_init"));
        assert!(
            c_header.contains(
                "void beskid_arch_v5_context_init(void * context, void * stack_top, void * entry, void * argument, void * return_trampoline);"
            )
        );
        assert!(c_header.contains("void beskid_arch_v5_context_switch(void * from, void * to);"));
        match manifest.target.triple.as_str() {
            "x86_64-unknown-linux-gnu" => {
                assert!(asm_include.contains("BESKID_X86_64_UNKNOWN_LINUX_GNU_STACK_ALIGNMENT = 16"));
                assert!(!asm_include.contains("AARCH64"));
                assert!(!asm_include.contains("WINDOWS"));
            }
            "aarch64-apple-darwin" => {
                assert!(asm_include.contains("BESKID_AARCH64_APPLE_DARWIN_CONTEXT_SIZE = 176"));
                assert!(!asm_include.contains("X86_64"));
                assert!(!asm_include.contains("stack+40"));
            }
            "x86_64-pc-windows-msvc" => {
                assert!(asm_include.contains("BESKID_X86_64_PC_WINDOWS_MSVC_SHADOW_SPACE EQU 32"));
                assert!(
                    asm_include.contains("BESKID_CONTEXT_INIT_RETURN_TRAMPOLINE_STACK_OPERAND TEXTEQU <[rsp + 40]>")
                );
                assert!(!asm_include.contains("AARCH64"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn assembly_init_signature_uses_stack_top_not_a_hardcoded_size() {
    for target in supported_targets() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let init = manifest
            .assembly_exports
            .iter()
            .find(|entry| entry.symbol == AssemblySymbol::new("beskid_arch_v5_context_init"))
            .unwrap();
        assert_eq!(init.params, vec![AbiType::Pointer; 5]);
        assert_eq!(init.result, AbiType::Void);
    }
}

fn artifact(path: &str) -> RuntimeArtifact {
    RuntimeArtifact {
        relative_path: path.into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    }
}

fn runtime_kit() -> RuntimeKitMetadata {
    let abi_contract = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let source_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let audit = RuntimeAuditMetadata::for_manifest(&abi_contract, source_hash).unwrap();
    RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target: abi_contract.target.clone(),
        profile: BuildProfile::Debug,
        layout_hash: abi_contract.layout_hash(),
        source_hash: source_hash.into(),
        artifacts: RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.so"),
            shared_import_library: None,
        },
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract,
        audit,
    }
}

#[test]
fn runtime_kit_abi_json_embeds_the_single_contract_and_generated_audit_metadata() {
    let metadata = runtime_kit();
    metadata.validate().expect("coherent runtime kit");
    assert_eq!(metadata.canonical_abi_json().unwrap(), metadata.canonical_abi_json().unwrap());
    let json = serde_json::to_value(&metadata).unwrap();
    assert_eq!(json["abi_contract"]["abi_version"], ABI_V5);
    assert_eq!(json["abi_contract"]["trusted_runtime_package"]["name"], CANONICAL_RUNTIME_PACKAGE_NAME);
    assert_eq!(json["audit"]["layout_hash"], metadata.layout_hash);
    assert_eq!(json["audit"]["runtime_source_hash"], metadata.source_hash);
    assert!(metadata.audit.forbidden_rust_symbols.contains(&"rust".into()));
    assert!(metadata.audit.allowed_exports.contains(&"beskid_arch_v5_context_switch".into()));

    let mut layout_drift = metadata.clone();
    layout_drift.layout_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    assert!(matches!(layout_drift.validate(), Err(RuntimeKitValidationError::ContractLayoutHashMismatch { .. })));

    let mut export_drift = metadata.clone();
    export_drift.export_allowlist.pop();
    assert!(matches!(export_drift.validate(), Err(RuntimeKitValidationError::ContractAuditMismatch { .. })));

    let mut source_drift = metadata;
    source_drift.audit.runtime_source_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into();
    assert!(matches!(source_drift.validate(), Err(RuntimeKitValidationError::ContractSourceHashMismatch { .. })));

    let mut missing_trust = runtime_kit();
    missing_trust.abi_contract.trusted_runtime_package = None;
    assert!(matches!(missing_trust.validate(), Err(RuntimeKitValidationError::InvalidAbiContract)));
}

#[test]
fn audit_metadata_rejects_unknown_duplicate_and_rust_provenance_contracts() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let audit = RuntimeAuditMetadata::for_manifest(
        &manifest,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .unwrap();
    audit.validate(&manifest).expect("generated audit metadata");

    let mut duplicate = audit.clone();
    duplicate.allowed_imports.push(duplicate.allowed_imports[0].clone());
    assert!(duplicate.validate(&manifest).is_err());

    let mut unknown = audit.clone();
    unknown.allowed_imports.push("mystery_allocator".into());
    assert!(unknown.validate(&manifest).is_err());

    let elf_undefined = audit.allowed_imports.iter().map(|symbol| format!("{symbol}@GLIBC_2.2.5")).collect::<Vec<_>>();
    assert!(
        audit
            .audit_object_symbol_tables(
                audit.allowed_exports.iter().map(String::as_str),
                elf_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );

    let mut missing_rust_guard = audit;
    missing_rust_guard.forbidden_rust_symbols.retain(|symbol| symbol != "rust");
    assert!(missing_rust_guard.validate(&manifest).is_err());

    let macho = AbiManifestV5::canonical_runtime(supported_targets()[1].clone());
    let macho_audit =
        RuntimeAuditMetadata::for_manifest(&macho, "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
            .unwrap();
    let macho_defined = macho_audit.allowed_exports.iter().map(|symbol| format!("_{symbol}")).collect::<Vec<_>>();
    let macho_undefined = macho_audit.allowed_imports.iter().map(|symbol| format!("_{symbol}")).collect::<Vec<_>>();
    assert!(
        macho_audit
            .audit_object_symbol_tables(
                macho_defined.iter().map(String::as_str),
                macho_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );
    for forbidden in [
        "___rust_alloc",
        "_core::panicking::panic_fmt",
        "_rust_eh_personality",
        "_abfall_switch",
        "__RNvCs1234_4core9panicking9panic_fmt",
        "__ZN4core9panicking9panic_fmt17h0123456789abcdefE",
    ] {
        let mut defined = macho_defined.clone();
        defined.push(forbidden.into());
        assert!(
            macho_audit
                .audit_object_symbol_tables(
                    defined.iter().map(String::as_str),
                    macho_undefined.iter().map(String::as_str),
                )
                .is_err()
        );
    }

    let mut missing = macho_defined.clone();
    missing.pop();
    assert!(
        macho_audit
            .audit_object_symbol_tables(missing.iter().map(String::as_str), macho_undefined.iter().map(String::as_str),)
            .is_err()
    );

    let windows = AbiManifestV5::canonical_runtime(supported_targets()[2].clone());
    let windows_audit = RuntimeAuditMetadata::for_manifest(
        &windows,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .unwrap();
    let windows_undefined =
        windows_audit.allowed_imports.iter().map(|symbol| format!("__imp_{symbol}")).collect::<Vec<_>>();
    assert!(
        windows_audit
            .audit_object_symbol_tables(
                windows_audit.allowed_exports.iter().map(String::as_str),
                windows_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );
}

#[test]
fn abi_json_rejects_unknown_fields() {
    let metadata = runtime_kit();
    let mut json = serde_json::to_value(&metadata).unwrap();
    json.as_object_mut().unwrap().insert("surprise".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<RuntimeKitMetadata>(json).is_err());

    let mut contract = serde_json::to_value(&metadata.abi_contract).unwrap();
    contract.as_object_mut().unwrap().insert("surprise".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<AbiManifestV5>(contract).is_err());
}

fn network_status_values() -> Vec<(String, i64)> {
    let source: serde_json::Value =
        serde_json::from_str(beskid_abi::generated::abi_v5_contract::ABI_V5_SOURCE_JSON).unwrap();
    let status = source["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|status| status["name"] == "BeskidNetworkStatus")
        .expect("manifest declares BeskidNetworkStatus");
    status["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| (value["name"].as_str().unwrap().to_owned(), value["value"].as_i64().unwrap()))
        .collect()
}

fn screaming_snake(name: &str) -> String {
    let mut output = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index != 0 {
            output.push('_');
        }
        output.push(character.to_ascii_uppercase());
    }
    output
}

fn workspace_source(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn network_status_manifest_c_enum_runtime_constants_and_corelib_errors_agree() {
    let statuses = network_status_values();
    for (index, (_, value)) in statuses.iter().enumerate() {
        assert_eq!(*value, index as i64, "network statuses are dense and positional");
    }
    let names = statuses.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert_eq!(names.first(), Some(&"Ok"));
    assert_eq!(names.last(), Some(&"Pending"), "exactly one internal pending status follows the public statuses");
    assert_eq!(names.iter().filter(|name| **name == "Pending").count(), 1);
    let value = |name: &str| statuses.iter().find(|(status, _)| status == name).unwrap().1;
    assert_eq!((value("NetworkDown"), value("ResourceExhausted"), value("Pending")), (9, 18, 19));

    let header = workspace_source("crates/beskid_abi/assembly/common/network.h");
    let start = header.find("enum { NET_OK").expect("network.h declares the positional status enum");
    let end = start + header[start..].find("};").unwrap();
    let c_names = header[start + "enum {".len()..end]
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let expected = names.iter().map(|name| format!("NET_{}", screaming_snake(name))).collect::<Vec<_>>();
    assert_eq!(c_names, expected, "network.h status enum mirrors the manifest positionally");
    assert!(header.contains("_Static_assert(NET_RESOURCE_EXHAUSTED == 18 && NET_PENDING == 19"));

    let public = &names[1..names.len() - 1];
    let errors = workspace_source("corelib/packages/network/src/Network/Errors.bd");
    let body = &errors[errors.find("pub enum NetworkError {").unwrap()..];
    let body = &body[body.find('{').unwrap() + 1..body.find('}').unwrap()];
    let variants = body
        .split(',')
        .map(|variant| variant.trim().trim_end_matches("()"))
        .filter(|variant| !variant.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(variants, public, "NetworkError declaration order is the status order");

    let internal = workspace_source("corelib/packages/network/src/Network/Internal.bd");
    for (name, status) in statuses.iter().filter(|(name, _)| public.contains(&name.as_str())) {
        let line = format!("if status == {status}_i64 {{ return NetworkError::{name}; }}");
        assert!(internal.contains(&line), "Network.Internal.Error maps status {status} to {name}");
    }
    assert!(!internal.contains(&format!("if status == {}_i64 {{ return NetworkError::", value("Pending"))));

    let table = workspace_source("runtime/beskid/src/Runtime/Network/Table.bd");
    let mut checked = 0;
    for line in table.lines().filter_map(|line| line.strip_prefix("const NETWORK_")) {
        let (constant, rest) = line.split_once(" = ").unwrap();
        let number = rest.trim_end_matches(';').parse::<i64>().unwrap();
        if let Some((_, expected)) = statuses.iter().find(|(name, _)| screaming_snake(name) == constant) {
            assert_eq!(number, *expected, "Table.bd NETWORK_{constant} matches the manifest");
            checked += 1;
        }
    }
    assert!(table.contains("const NETWORK_RESOURCE_EXHAUSTED = 18;") && table.contains("const NETWORK_PENDING = 19;"));
    assert!(checked >= 10, "Table.bd names its status constants after manifest statuses");
    assert!(!table.contains("const NETWORK_DOWN"), "runtime-owned failures never report NetworkDown");
}
