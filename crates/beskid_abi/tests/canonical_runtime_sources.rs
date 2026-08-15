use beskid_abi::abi_v5::{AbiManifestV5, AbiType, SourceUnit, TargetMetadata};
use beskid_abi::runtime_source::{
    canonical_corelib_service_capability, canonical_corelib_service_source_path,
    canonical_runtime_intrinsic_capability, canonical_runtime_sources, prove_canonical_runtime_corpus,
    RuntimeCapabilityError, CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
    CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_SOURCE_PATH,
    CANONICAL_CLOCKS_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH, CANONICAL_PROCESS_SOURCE_PATH, CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH,
    CANONICAL_SCHEDULER_LOOP_SOURCE_PATH, CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH,
    CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH, CANONICAL_SYSCALLS_SOURCE_PATH,
};

fn canonical_source<'a>(sources: &'a [SourceUnit], logical_path: &str) -> &'a str {
    &sources
        .iter()
        .find(|unit| unit.logical_path == logical_path)
        .unwrap_or_else(|| panic!("canonical source {logical_path}"))
        .source
}

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

#[test]
fn canonical_bootstrap_facade_routes_to_one_ordered_source_per_responsibility() {
    let sources = canonical_runtime_sources();
    let paths = sources.iter().map(|unit| unit.logical_path.as_str()).collect::<Vec<_>>();
    let bootstrap_index =
        paths.iter().position(|path| *path == CANONICAL_BOOTSTRAP_SOURCE_PATH).expect("canonical Bootstrap facade");
    assert_eq!(
        &paths[bootstrap_index..bootstrap_index + 5],
        [
            CANONICAL_BOOTSTRAP_SOURCE_PATH,
            CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
            CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH,
            CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH,
            CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH,
        ]
    );

    let facade = canonical_source(&sources, CANONICAL_BOOTSTRAP_SOURCE_PATH);
    for (module, logical_path) in [
        ("Runtime.Bootstrap.Native", CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH),
        ("Runtime.Bootstrap.Lifecycle", CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH),
        ("Runtime.Bootstrap.Roots", CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH),
        ("Runtime.Bootstrap.Objects", CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH),
    ] {
        assert_eq!(facade.matches(&format!("pub mod {module};")).count(), 1);
        assert_eq!(paths.iter().filter(|path| **path == logical_path).count(), 1);
    }
}

#[test]
fn canonical_scheduler_facade_routes_to_one_ordered_source_per_responsibility() {
    let sources = canonical_runtime_sources();
    let paths = sources.iter().map(|unit| unit.logical_path.as_str()).collect::<Vec<_>>();
    let scheduler_index =
        paths.iter().position(|path| *path == CANONICAL_SCHEDULER_SOURCE_PATH).expect("canonical Scheduler facade");
    assert_eq!(
        &paths[scheduler_index..scheduler_index + 7],
        [
            CANONICAL_SCHEDULER_SOURCE_PATH,
            CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
            CANONICAL_SCHEDULER_CORE_SOURCE_PATH,
            CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH,
            CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH,
            CANONICAL_SCHEDULER_LOOP_SOURCE_PATH,
            CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH,
        ]
    );

    let facade = canonical_source(&sources, CANONICAL_SCHEDULER_SOURCE_PATH);
    for (module, logical_path) in [
        ("Runtime.Fiber.Scheduler.Context", CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH),
        ("Runtime.Fiber.Scheduler.Core", CANONICAL_SCHEDULER_CORE_SOURCE_PATH),
        ("Runtime.Fiber.Scheduler.Storage", CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH),
        ("Runtime.Fiber.Scheduler.Queue", CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH),
        ("Runtime.Fiber.Scheduler.Loop", CANONICAL_SCHEDULER_LOOP_SOURCE_PATH),
        ("Runtime.Fiber.Scheduler.Exports", CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH),
    ] {
        assert_eq!(facade.matches(&format!("pub mod {module};")).count(), 1);
        assert_eq!(paths.iter().filter(|path| **path == logical_path).count(), 1);
    }
}

#[test]
fn canonical_bootstrap_source_is_embedded_and_exports_the_v5_probe() {
    let sources = canonical_runtime_sources();
    let lifecycle = canonical_source(&sources, CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH);
    assert!(lifecycle.contains("[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_abi_version\")]"));
    assert!(lifecycle.contains("return 5;"));
    assert!(!lifecycle.contains("ABI v4"));
    assert!(!lifecycle.contains("__"));
}

#[test]
fn canonical_bootstrap_source_exports_the_v5_lifecycle_and_trap_wrappers() {
    let sources = canonical_runtime_sources();
    let lifecycle = canonical_source(&sources, CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH);
    let native = canonical_source(&sources, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH);

    // Every manifest export is owned by some canonical runtime source. Ownership is corpus-wide,
    // not bootstrap-only: scheduler exports such as the fiber spawn entry live in the scheduler
    // unit. Context-switch exports are intentionally supplied by target assembly and covered by
    // their own assembly tests.
    for export in &linux_manifest().exports {
        let declaration = format!("Symbol:\"{}\"", export.symbol);
        assert!(
            sources.iter().any(|unit| unit.source.contains(&declaration)),
            "canonical runtime corpus must own manifest export {}",
            export.symbol,
        );
    }

    for symbol in [
        "beskid_library_attach_v5",
        "beskid_library_detach_v5",
        "beskid_rt_v5_process_init",
        "beskid_rt_v5_process_shutdown",
        "beskid_rt_v5_thread_attach",
        "beskid_rt_v5_thread_detach",
    ] {
        assert!(lifecycle.contains(&format!("Symbol:\"{symbol}\"")), "canonical lifecycle source must own {symbol}",);
    }
    assert!(native.contains("Symbol:\"beskid_rt_v5_trap\""));

    assert!(lifecycle.contains("pub pointer ProcessInit(pointer config)"));
    assert!(lifecycle.contains("pub unit ProcessShutdown(pointer runtime)"));
    assert!(lifecycle.contains("pub i32 LibraryAttach(pointer runtime)"));
    assert!(lifecycle.contains("pub unit LibraryDetach(pointer runtime)"));
    assert!(lifecycle.contains("pub pointer ThreadAttach(pointer runtime)"));
    assert!(lifecycle.contains("pub unit ThreadDetach(pointer thread)"));
    assert!(native.contains("pub never Trap(u8 code, pointer message, word messageLength)"));
    assert!(native.contains("trap(code, message, messageLength);"));
}

#[test]
fn canonical_bootstrap_source_uses_only_manifest_owned_allocation_and_tls_primitives() {
    let sources = canonical_runtime_sources();
    let native = canonical_source(&sources, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH);
    let roots = canonical_source(&sources, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH);

    assert!(native.contains("pub pointer SystemAllocate(word size, word alignment)"));
    assert!(native.contains("return system_allocate(size, alignment);"));
    assert!(native.contains("pub unit SystemFree(pointer address, word size)"));
    assert!(native.contains("system_free(address, size);"));

    assert!(native.contains("pub pointer CurrentThreadState()"));
    assert!(native.contains("return tls_get();"));
    assert!(native.contains("pub unit SetCurrentThreadState(pointer state)"));
    assert!(native.contains("tls_set(state);"));

    assert!(roots.contains("pub pointer RootFrame(pointer tlsState)"));
    assert!(roots.contains("raw_word_load(pointer_add(tlsState, 8))"));
    assert!(roots.contains("pub unit SetRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(roots.contains("raw_word_store(pointer_add(tlsState, 8), NativeWord(rootFrame));"));
}

#[test]
fn canonical_host_sources_use_manifest_owned_clock_and_process_adapters() {
    let manifest = linux_manifest();
    for (name, symbol, capability, result) in [
        (
            "clock_monotonic_nanos",
            "beskid_rt_v5_intrinsic_clock_monotonic_nanos",
            "runtime.adapter.clock_monotonic_nanos",
            AbiType::I64,
        ),
        (
            "clock_realtime_nanos",
            "beskid_rt_v5_intrinsic_clock_realtime_nanos",
            "runtime.adapter.clock_realtime_nanos",
            AbiType::I64,
        ),
        ("process_exit", "beskid_rt_v5_intrinsic_process_exit", "runtime.adapter.process_exit", AbiType::Void),
        ("process_getpid", "beskid_rt_v5_intrinsic_process_getpid", "runtime.adapter.process_getpid", AbiType::I32),
    ] {
        let intrinsic = manifest
            .trusted_runtime_intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == name)
            .unwrap_or_else(|| panic!("manifest must declare {name}"));
        assert_eq!(intrinsic.symbol, symbol);
        assert_eq!(intrinsic.capability, capability);
        assert_eq!(intrinsic.result, result);
        assert_eq!(intrinsic.noreturn, name == "process_exit");
    }

    let sources = canonical_runtime_sources();
    let clocks = &sources
        .iter()
        .find(|unit| unit.logical_path == CANONICAL_CLOCKS_SOURCE_PATH)
        .expect("canonical clock source")
        .source;
    assert!(clocks.contains("return clock_monotonic_nanos();"));
    assert!(clocks.contains("return clock_realtime_nanos();"));

    let process = &sources
        .iter()
        .find(|unit| unit.logical_path == CANONICAL_PROCESS_SOURCE_PATH)
        .expect("canonical process source")
        .source;
    assert!(process.contains("process_exit(code);"));
    assert!(process.contains("return process_getpid();"));
    assert!(process.contains("pub pointer EnvGet(pointer key) { return env_get(key); }"));
    assert!(process.contains("pub i32 EnvSet(pointer key, pointer value) { return env_set(key, value); }"));
    assert!(process.contains("pub pointer EnvGetcwd() { return env_getcwd(); }"));
    assert!(process.contains("i32 status = fs_read_text(path, result, pointer_add(result, 8));"));
    assert!(process.contains("pointer text = StrNew(bytes, length);"));
    assert!(process.contains("fs_read_text_release(bytes, length);"));
    assert!(process.contains("raw_word_store(textOut, raw_word_load(text));"));
    assert!(
        process.contains("pub i32 FsWriteText(pointer path, pointer content) { return fs_write_text(path, content); }")
    );
    assert!(process.contains("pub i32 FsExists(pointer path) { return fs_exists(path); }"));
    assert!(process.contains("pub i32 FsMkdir(pointer path) { return fs_mkdir(path); }"));
    assert!(process.contains("pub i32 FsDelete(pointer path) { return fs_delete(path); }"));
    assert!(process.contains("pub pointer TtyWinsize() { return tty_winsize(); }"));

    let syscalls = canonical_source(&sources, CANONICAL_SYSCALLS_SOURCE_PATH);
    assert!(syscalls.contains("return i64(write(fd, buffer, len));"));
    assert!(syscalls.contains("return i64(read(fd, buffer, len));"));
    assert!(!syscalls.contains("return -1;"));
}

#[test]
fn canonical_bootstrap_owns_beskid_tls_state_on_thread_attach_detach() {
    let sources = canonical_runtime_sources();
    let lifecycle = canonical_source(&sources, CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH);

    // ProcessInit stamps BeskidRuntimeState.abi_version at offset 0 and must not install the
    // RuntimeState pointer as if it were BeskidTlsState (root_frame lives at TLS offset 8).
    assert!(lifecycle.contains("raw_word_store(config, 5)"));
    assert!(!lifecycle.contains("tls_set(config);"), "ProcessInit must not put BeskidRuntimeState into TLS");

    // ThreadAttach allocates a dedicated BeskidTlsState (size 32, alignment 8).
    assert!(lifecycle.contains("SystemAllocate(32, 8)"));
    assert!(lifecycle.contains("memory_set(tlsState, 0, 32)"));
    assert!(lifecycle.contains("raw_word_store(tlsState, NativeWord(runtime))"));
    assert!(lifecycle.contains("raw_word_store(pointer_add(tlsState, 8), 0)"));
    assert!(lifecycle.contains("raw_word_store(pointer_add(tlsState, 16), 0)"));
    assert!(lifecycle.contains("raw_word_store(pointer_add(tlsState, 24), 1)"));
    assert!(lifecycle.contains("raw_word_store(pointer_add(runtime, 8), NativeWord(tlsState))"));
    assert!(lifecycle.contains("SetCurrentThreadState(tlsState)"));
    assert!(lifecycle.contains("return tlsState;"));

    // Matching detach clears TLS + RuntimeState.current_thread and frees the 32-byte record.
    assert!(lifecycle.contains("SetCurrentThreadState(NativePointer(0))"));
    assert!(lifecycle.contains("raw_word_store(pointer_add(runtime, 8), 0)"));
    assert!(lifecycle.contains("SystemFree(thread, 32)"));
}

#[test]
fn canonical_runtime_source_owns_allocation_headers_and_lifo_root_frames() {
    let sources = canonical_runtime_sources();
    let objects = canonical_source(&sources, CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH);
    let roots = canonical_source(&sources, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH);

    // The bridge's `allocate_beskid` returns the address of an object header, zero-fills the
    // allocation, and installs the descriptor at offset zero.  The source runtime must retain
    // that ownership shape before a collector is allowed to trace or sweep it.
    assert!(objects.contains("pub word AllocationSize(pointer request)"));
    assert!(objects.contains("raw_word_load(request)"));
    assert!(objects.contains("pub pointer AllocationDescriptor(pointer request)"));
    assert!(objects.contains("pointer_add(request, 16)"));
    assert!(objects.contains("pub pointer AllocateObject(pointer request)"));
    assert!(objects.contains("memory_set(object, 0, size);"));
    assert!(objects.contains("InitializeObjectHeader(object, descriptor);"));
    assert!(objects.contains("pub unit ReleaseObject(pointer object, pointer request)"));

    // Frames are caller-owned stack records.  The runtime only links and unlinks them in LIFO
    // order; scanning and collection deliberately remain separate future responsibilities.
    assert!(roots.contains("pub pointer RootFramePrevious(pointer rootFrame)"));
    assert!(roots.contains("pub pointer RootFrameSlots(pointer rootFrame)"));
    assert!(roots.contains("pub word RootFrameSlotCount(pointer rootFrame)"));
    assert!(roots.contains("pub unit PushRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(roots.contains("pub bool PopRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(roots.contains("if RootFrame(tlsState) != rootFrame"));
    assert!(roots.contains("SetRootFrame(tlsState, RootFramePrevious(rootFrame));"));
    assert!(!roots.contains("CollectGarbage("));
    assert!(!roots.contains("Sweep("));
}

#[test]
fn canonical_gc_exports_one_registry_backed_external_root_count() {
    let sources = canonical_runtime_sources();
    let roots_handles = &sources
        .iter()
        .find(|unit| unit.logical_path == CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH)
        .expect("canonical GC roots and handles source")
        .source;

    assert_eq!(
        sources
            .iter()
            .map(|unit| unit.source.matches("[Export(Abi:\"C\", Symbol:\"gc_external_root_count\")]").count())
            .sum::<usize>(),
        1,
        "the canonical runtime must define one gc_external_root_count ABI export",
    );
    assert!(roots_handles.contains("return raw_word_load(pointer_add(heap, ROOT_REGISTRY_OFFSET - 8));"));
    assert!(!roots_handles.contains("count from handles table"));
}

#[test]
fn canonical_runtime_source_fail_closes_closure_descriptors_before_allocation_and_rooting() {
    let sources = canonical_runtime_sources();
    let objects = canonical_source(&sources, CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH);
    let roots = canonical_source(&sources, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH);

    // A descriptor is a 40-byte ABI record. Pointer-map entries are byte offsets into the
    // complete object, so validation must reject a non-word offset and all arithmetic must be
    // checked before forming an address. Overflow uses wrapping multiply/add (not
    // `NativeWordMax() - 8`) because `word` compares are signed in Cranelift.
    assert!(objects.contains("pub bool ValidatePointerMap(pointer pointerMap, word pointerCount, word objectSize)"));
    assert!(objects.contains("word mapOffset = index * 8"));
    assert!(objects.contains("if mapOffset / 8 != index"));
    assert!(objects.contains("if offset % 8 != 0"));
    assert!(objects.contains("word end = offset + 8"));
    assert!(objects.contains("if end < offset"));
    assert!(objects.contains("if end > objectSize"));

    // ABI allocations require a power-of-two word alignment, not merely a value >= 8.
    assert!(objects.contains("pub bool IsValidObjectAlignment(word alignment)"));
    assert!(objects.contains("if alignment < 8"));
    assert!(objects.contains("if remaining % 2 != 0"));

    // The manifest-owned managed-object path is the only descriptor-backed allocator. It must
    // validate the complete request before reserving storage and initialize the ABI header.
    assert!(objects.contains(
        "[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_managed_object_allocate\")]\npub pointer AllocateObject(pointer request)"
    ));
    let allocate = objects
        .split("pub pointer AllocateObject(pointer request)")
        .nth(1)
        .expect("managed allocation function")
        .split("// Allocates a closure capture environment")
        .next()
        .expect("managed allocation body");
    let null_guard = allocate.find("if request == NativePointer(0)").expect("null request guard");
    let descriptor_read = allocate.find("AllocationDescriptor(request)").expect("descriptor read");
    assert!(null_guard < descriptor_read);
    assert!(allocate.contains("bool descriptorOk = ValidateTypeDescriptor(descriptor);"));
    assert!(allocate.contains("if descriptorOk"));
    assert!(allocate.contains("if size != TypeDescriptorSize(descriptor)"));
    assert!(allocate.contains("if alignment != TypeDescriptorAlignment(descriptor)"));
    assert!(allocate.contains("InitializeObjectHeader(object, descriptor);"));

    // The compatibility closure export delegates to the same implementation. It must not retain
    // a second validation, allocation, zeroing, or header-initialization path.
    let closure_allocate = objects
        .split("pub pointer AllocateClosureEnvironment(pointer request)")
        .nth(1)
        .expect("closure allocation function")
        .split("// Stores a capture")
        .next()
        .expect("closure allocation body");
    assert!(closure_allocate.contains("return AllocateObject(request);"));
    for duplicate in ["ValidateTypeDescriptor", "SystemAllocate", "memory_set", "InitializeObjectHeader"] {
        assert!(!closure_allocate.contains(duplicate), "closure allocation must not duplicate {duplicate}");
    }

    assert!(objects.contains(
        "pub bool StoreClosureCapture(pointer environment, pointer descriptor, word mapIndex, pointer value)"
    ));
    assert!(roots.contains("pub bool RootClosureEnvironment(pointer tlsState, word slotIndex, pointer environment)"));
    assert!(roots.contains("return SetRootSlotValue(rootFrame, slotIndex, environment);"));
    assert!(roots.contains("pub bool RootClosureEnvironmentCurrent(word slotIndex, pointer environment)"));
    assert!(roots.contains("return RootClosureEnvironment(CurrentThreadState(), slotIndex, environment);"));
}

#[test]
fn exact_embedded_source_set_receives_non_serializable_intrinsic_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let proof = prove_canonical_runtime_corpus(&sources, &manifest).expect("canonical source proof");
    let capability = canonical_runtime_intrinsic_capability(&manifest).expect("canonical intrinsic authority");

    for logical_path in [
        CANONICAL_BOOTSTRAP_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH,
    ] {
        assert!(proof.authorizes_source(logical_path));
        assert!(capability.authorizes_source(logical_path));
    }
    for intrinsic in [
        "system_allocate",
        "system_free",
        "tls_get",
        "tls_set",
        "pointer_add",
        "raw_word_load",
        "raw_word_store",
        "memory_set",
        "trap",
    ] {
        assert!(
            capability.intrinsic_for_source(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, intrinsic).is_some(),
            "canonical runtime must retain authority for {intrinsic}",
        );
    }
    assert!(capability.intrinsic_for_source("src/User.bd", "trap").is_none());
    assert!(capability
        .intrinsic_for_source(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, "not_manifest_declared",)
        .is_none());
    assert_eq!(capability.source_hash(), beskid_abi::abi_v5::canonical_source_hash(&sources).unwrap());
}

#[test]
fn lookalike_source_path_name_or_contents_cannot_receive_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();

    let mut changed = sources.clone();
    changed[0].source.push_str("\n// drift\n");
    assert!(matches!(
        prove_canonical_runtime_corpus(&changed, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));

    let mut renamed = sources.clone();
    renamed[0].logical_path = "src/User/Bootstrap.bd".into();
    assert!(matches!(
        prove_canonical_runtime_corpus(&renamed, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));

    let mut extra = sources;
    extra.push(SourceUnit {
        logical_path: "src/Runtime/Backdoor.bd".into(),
        source: "pub unit Backdoor() { return; }".into(),
    });
    assert!(matches!(
        prove_canonical_runtime_corpus(&extra, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));
}

#[test]
fn manifest_drift_cannot_expand_runtime_authority() {
    let mut manifest = linux_manifest();
    manifest.trusted_runtime_intrinsics.pop();
    assert!(matches!(canonical_runtime_intrinsic_capability(&manifest), Err(RuntimeCapabilityError::InvalidManifest)));
}

#[test]
fn canonical_foundation_assert_owns_only_the_panic_service() {
    let capability = canonical_corelib_service_capability(&linux_manifest()).expect("Corelib service authority");

    assert_eq!(
        capability
            .service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__panic_str")
            .map(|service| service.symbol),
        Some("panic_str")
    );
    assert!(
        capability.service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__syscall_write").is_none(),
        "the Assert unit must not receive every Corelib service"
    );
}

#[test]
fn canonical_corelib_service_source_paths_are_lexically_normalized() {
    for logical in [CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH] {
        let path =
            canonical_corelib_service_source_path(logical).unwrap_or_else(|| panic!("missing path for {logical}"));
        assert!(
            !path.components().any(|component| matches!(component, std::path::Component::ParentDir)),
            "service path for {logical} retained ParentDir: {path:?}"
        );
        assert!(
            path.ends_with(std::path::Path::new(logical)),
            "normalized path for {logical} lost its relative suffix: {path:?}"
        );
    }
}
