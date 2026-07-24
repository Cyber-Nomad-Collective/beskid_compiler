use beskid_abi::abi_v5::{AbiManifestV5, SourceUnit, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, RuntimeCapabilityError,
    canonical_corelib_service_capability, canonical_corelib_service_source_path,
    canonical_runtime_intrinsic_capability, canonical_runtime_sources,
    prove_canonical_runtime_corpus,
};

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

#[test]
fn canonical_bootstrap_source_is_embedded_and_exports_the_v5_probe() {
    let sources = canonical_runtime_sources();
    let bootstrap = sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source");
    assert!(
        bootstrap
            .source
            .contains("[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_abi_version\")]")
    );
    assert!(bootstrap.source.contains("return 5;"));
    assert!(!bootstrap.source.contains("ABI v4"));
    assert!(!bootstrap.source.contains("__"));
}

#[test]
fn canonical_bootstrap_source_exports_the_v5_lifecycle_and_trap_wrappers() {
    let sources = canonical_runtime_sources();
    let source = &sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source")
        .source;

    // Every manifest lifecycle/trap export is source-owned. Context-switch exports are
    // intentionally supplied by target assembly and covered by their own assembly tests.
    for export in &linux_manifest().exports {
        assert!(
            source.contains(&format!("Symbol:\"{}\"", export.symbol)),
            "canonical runtime source must own manifest export {}",
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
        "beskid_rt_v5_trap",
    ] {
        assert!(
            source.contains(&format!("Symbol:\"{symbol}\"")),
            "canonical runtime source must own {symbol}",
        );
    }

    assert!(source.contains("pub pointer ProcessInit(pointer config)"));
    assert!(source.contains("pub unit ProcessShutdown(pointer runtime)"));
    assert!(source.contains("pub i32 LibraryAttach(pointer runtime)"));
    assert!(source.contains("pub unit LibraryDetach(pointer runtime)"));
    assert!(source.contains("pub pointer ThreadAttach(pointer runtime)"));
    assert!(source.contains("pub unit ThreadDetach(pointer thread)"));
    assert!(source.contains("pub never Trap(u8 code, pointer message, word messageLength)"));
    assert!(source.contains("trap(code, message, messageLength);"));
}

#[test]
fn canonical_bootstrap_source_uses_only_manifest_owned_allocation_and_tls_primitives() {
    let sources = canonical_runtime_sources();
    let source = &sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source")
        .source;

    assert!(source.contains("pub pointer SystemAllocate(word size, word alignment)"));
    assert!(source.contains("return system_allocate(size, alignment);"));
    assert!(source.contains("pub unit SystemFree(pointer address, word size)"));
    assert!(source.contains("system_free(address, size);"));

    assert!(source.contains("pub pointer CurrentThreadState()"));
    assert!(source.contains("return tls_get();"));
    assert!(source.contains("pub unit SetCurrentThreadState(pointer state)"));
    assert!(source.contains("tls_set(state);"));

    assert!(source.contains("pub pointer RootFrame(pointer tlsState)"));
    assert!(source.contains("raw_word_load(pointer_add(tlsState, 8))"));
    assert!(source.contains("pub unit SetRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(source.contains("raw_word_store(pointer_add(tlsState, 8), NativeWord(rootFrame));"));
}

#[test]
fn canonical_bootstrap_owns_beskid_tls_state_on_thread_attach_detach() {
    let sources = canonical_runtime_sources();
    let source = &sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source")
        .source;

    // ProcessInit stamps BeskidRuntimeState.abi_version at offset 0 and must not install the
    // RuntimeState pointer as if it were BeskidTlsState (root_frame lives at TLS offset 8).
    assert!(source.contains("raw_word_store(config, 5)"));
    assert!(
        !source.contains("tls_set(config);"),
        "ProcessInit must not put BeskidRuntimeState into TLS"
    );

    // ThreadAttach allocates a dedicated BeskidTlsState (size 32, alignment 8).
    assert!(source.contains("SystemAllocate(32, 8)"));
    assert!(source.contains("memory_set(tlsState, 0, 32)"));
    assert!(source.contains("raw_word_store(tlsState, NativeWord(runtime))"));
    assert!(source.contains("raw_word_store(pointer_add(tlsState, 8), 0)"));
    assert!(source.contains("raw_word_store(pointer_add(tlsState, 16), 0)"));
    assert!(source.contains("raw_word_store(pointer_add(tlsState, 24), 1)"));
    assert!(source.contains("raw_word_store(pointer_add(runtime, 8), NativeWord(tlsState))"));
    assert!(source.contains("SetCurrentThreadState(tlsState)"));
    assert!(source.contains("return tlsState;"));

    // Matching detach clears TLS + RuntimeState.current_thread and frees the 32-byte record.
    assert!(source.contains("SetCurrentThreadState(NativePointer(0))"));
    assert!(source.contains("raw_word_store(pointer_add(runtime, 8), 0)"));
    assert!(source.contains("SystemFree(thread, 32)"));
}

#[test]
fn canonical_runtime_source_owns_allocation_headers_and_lifo_root_frames() {
    let sources = canonical_runtime_sources();
    let source = &sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source")
        .source;

    // The bridge's `allocate_beskid` returns the address of an object header, zero-fills the
    // allocation, and installs the descriptor at offset zero.  The source runtime must retain
    // that ownership shape before a collector is allowed to trace or sweep it.
    assert!(source.contains("pub word AllocationSize(pointer request)"));
    assert!(source.contains("raw_word_load(request)"));
    assert!(source.contains("pub pointer AllocationDescriptor(pointer request)"));
    assert!(source.contains("pointer_add(request, 16)"));
    assert!(source.contains("pub pointer AllocateObject(pointer request)"));
    assert!(source.contains("memory_set(object, 0, size);"));
    assert!(source.contains("InitializeObjectHeader(object, descriptor);"));
    assert!(source.contains("pub unit ReleaseObject(pointer object, pointer request)"));

    // Frames are caller-owned stack records.  The runtime only links and unlinks them in LIFO
    // order; scanning and collection deliberately remain separate future responsibilities.
    assert!(source.contains("pub pointer RootFramePrevious(pointer rootFrame)"));
    assert!(source.contains("pub pointer RootFrameSlots(pointer rootFrame)"));
    assert!(source.contains("pub word RootFrameSlotCount(pointer rootFrame)"));
    assert!(source.contains("pub unit PushRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(source.contains("pub bool PopRootFrame(pointer tlsState, pointer rootFrame)"));
    assert!(source.contains("if RootFrame(tlsState) != rootFrame"));
    assert!(source.contains("SetRootFrame(tlsState, RootFramePrevious(rootFrame));"));
    assert!(!source.contains("CollectGarbage("));
    assert!(!source.contains("Sweep("));
}

#[test]
fn canonical_runtime_source_fail_closes_closure_descriptors_before_allocation_and_rooting() {
    let sources = canonical_runtime_sources();
    let source = &sources
        .iter()
        .find(|u| u.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .expect("bootstrap source")
        .source;

    // A descriptor is a 40-byte ABI record. Pointer-map entries are byte offsets into the
    // complete object, so validation must reject a non-word offset and all arithmetic must be
    // checked before forming an address. Overflow uses wrapping multiply/add (not
    // `NativeWordMax() - 8`) because `word` compares are signed in Cranelift.
    assert!(source.contains("pub bool ValidatePointerMap(pointer pointerMap, word pointerCount, word objectSize)"));
    assert!(source.contains("word mapOffset = index * 8"));
    assert!(source.contains("if mapOffset / 8 != index"));
    assert!(source.contains("if offset % 8 != 0"));
    assert!(source.contains("word end = offset + 8"));
    assert!(source.contains("if end < offset"));
    assert!(source.contains("if end > objectSize"));

    // ABI allocations require a power-of-two word alignment, not merely a value >= 8.
    assert!(source.contains("pub bool IsValidObjectAlignment(word alignment)"));
    assert!(source.contains("if alignment < 8"));
    assert!(source.contains("if remaining % 2 != 0"));

    // The manifest-owned managed-object path is the only descriptor-backed allocator. It must
    // validate the complete request before reserving storage and initialize the ABI header.
    assert!(source.contains(
        "[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_managed_object_allocate\")]\npub pointer AllocateObject(pointer request)"
    ));
    let allocate = source
        .split("pub pointer AllocateObject(pointer request)")
        .nth(1)
        .expect("managed allocation function")
        .split("// Allocates a closure capture environment")
        .next()
        .expect("managed allocation body");
    let null_guard = allocate
        .find("if request == NativePointer(0)")
        .expect("null request guard");
    let descriptor_read = allocate
        .find("AllocationDescriptor(request)")
        .expect("descriptor read");
    assert!(null_guard < descriptor_read);
    assert!(allocate.contains("bool descriptorOk = ValidateTypeDescriptor(descriptor);"));
    assert!(allocate.contains("if descriptorOk"));
    assert!(allocate.contains("if size != TypeDescriptorSize(descriptor)"));
    assert!(allocate.contains("if alignment != TypeDescriptorAlignment(descriptor)"));
    assert!(allocate.contains("InitializeObjectHeader(object, descriptor);"));

    // The compatibility closure export delegates to the same implementation. It must not retain
    // a second validation, allocation, zeroing, or header-initialization path.
    let closure_allocate = source
        .split("pub pointer AllocateClosureEnvironment(pointer request)")
        .nth(1)
        .expect("closure allocation function")
        .split("// Stores a capture")
        .next()
        .expect("closure allocation body");
    assert!(closure_allocate.contains("return AllocateObject(request);"));
    for duplicate in [
        "ValidateTypeDescriptor",
        "SystemAllocate",
        "memory_set",
        "InitializeObjectHeader",
    ] {
        assert!(
            !closure_allocate.contains(duplicate),
            "closure allocation must not duplicate {duplicate}"
        );
    }

    assert!(source.contains("pub bool StoreClosureCapture(pointer environment, pointer descriptor, word mapIndex, pointer value)"));
    assert!(source.contains("pub bool RootClosureEnvironment(pointer tlsState, word slotIndex, pointer environment)"));
    assert!(source.contains("return SetRootSlotValue(rootFrame, slotIndex, environment);"));
    assert!(source.contains("pub bool RootClosureEnvironmentCurrent(word slotIndex, pointer environment)"));
    assert!(source.contains("return RootClosureEnvironment(CurrentThreadState(), slotIndex, environment);"));
}

#[test]
fn exact_embedded_source_set_receives_non_serializable_intrinsic_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let proof =
        prove_canonical_runtime_corpus(&sources, &manifest).expect("canonical source proof");
    let capability =
        canonical_runtime_intrinsic_capability(&manifest).expect("canonical intrinsic authority");

    assert!(proof.authorizes_source(CANONICAL_BOOTSTRAP_SOURCE_PATH));
    assert!(capability.authorizes_source(CANONICAL_BOOTSTRAP_SOURCE_PATH));
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
            capability
                .intrinsic_for_source(CANONICAL_BOOTSTRAP_SOURCE_PATH, intrinsic)
                .is_some(),
            "canonical runtime must retain authority for {intrinsic}",
        );
    }
    assert!(
        capability
            .intrinsic_for_source("src/User.bd", "trap")
            .is_none()
    );
    assert!(
        capability
            .intrinsic_for_source(CANONICAL_BOOTSTRAP_SOURCE_PATH, "not_manifest_declared",)
            .is_none()
    );
    assert_eq!(
        capability.source_hash(),
        beskid_abi::abi_v5::canonical_source_hash(&sources).unwrap()
    );
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
    assert!(matches!(
        canonical_runtime_intrinsic_capability(&manifest),
        Err(RuntimeCapabilityError::InvalidManifest)
    ));
}

#[test]
fn canonical_foundation_assert_owns_only_the_panic_service() {
    let capability =
        canonical_corelib_service_capability(&linux_manifest()).expect("Corelib service authority");

    assert_eq!(
        capability
            .service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__panic_str")
            .map(|service| service.symbol),
        Some("panic_str")
    );
    assert!(
        capability
            .service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__syscall_write")
            .is_none(),
        "the Assert unit must not receive every Corelib service"
    );
}

#[test]
fn canonical_corelib_service_source_paths_are_lexically_normalized() {
    for logical in [
        CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
        CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    ] {
        let path = canonical_corelib_service_source_path(logical)
            .unwrap_or_else(|| panic!("missing path for {logical}"));
        assert!(
            !path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir)),
            "service path for {logical} retained ParentDir: {path:?}"
        );
        assert!(
            path.ends_with(std::path::Path::new(logical)),
            "normalized path for {logical} lost its relative suffix: {path:?}"
        );
    }
}
