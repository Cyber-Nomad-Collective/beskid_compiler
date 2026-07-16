use beskid_abi::abi_v5::{AbiManifestV5, SourceUnit, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, RuntimeCapabilityError,
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
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].logical_path, CANONICAL_BOOTSTRAP_SOURCE_PATH);
    assert!(
        sources[0]
            .source
            .contains("[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_abi_version\")]")
    );
    assert!(sources[0].source.contains("return 5;"));
    assert!(!sources[0].source.contains("ABI v4"));
    assert!(!sources[0].source.contains("__"));
}

#[test]
fn canonical_bootstrap_source_exports_the_v5_lifecycle_and_trap_wrappers() {
    let source = &canonical_runtime_sources()[0].source;

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
    let source = &canonical_runtime_sources()[0].source;

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
fn canonical_runtime_source_owns_allocation_headers_and_lifo_root_frames() {
    let source = &canonical_runtime_sources()[0].source;

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
