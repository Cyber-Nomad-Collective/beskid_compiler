use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_source::{CANONICAL_SCHEDULER_SOURCE_PATH, canonical_runtime_sources},
};

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

#[test]
fn canonical_scheduler_context_boundary_is_manifest_derived() {
    let manifest = linux_manifest();
    let scheduler = canonical_runtime_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_SOURCE_PATH)
        .expect("canonical scheduler source")
        .source;

    for name in ["arch_context_size", "arch_context_alignment", "context_init", "context_switch"] {
        assert!(
            manifest.trusted_runtime_intrinsics.iter().any(|intrinsic| intrinsic.name == name),
            "canonical manifest must declare {name}",
        );
    }
    assert!(scheduler.contains("pub word ArchContextSize()"));
    assert!(scheduler.contains("return arch_context_size();"));
    assert!(scheduler.contains("pub word ArchContextAlignment()"));
    assert!(scheduler.contains("return arch_context_alignment();"));
    assert!(scheduler.contains("pub unit ContextInit("));
    assert!(scheduler.contains("context_init(context, stackTop, entry, argument, returnTrampoline);"));
    assert!(scheduler.contains("pub unit ContextSwitch(pointer from, pointer to)"));
    assert!(scheduler.contains("context_switch(from, to);"));
    assert!(scheduler.contains("pointer context = SystemAllocate(ArchContextSize(), ArchContextAlignment());"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 104), NativeWord(context));"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 112), ArchContextSize());"));
}

#[test]
fn canonical_scheduler_uses_manifest_guarded_stacks_with_bounded_usable_storage() {
    let manifest = linux_manifest();
    let scheduler = canonical_runtime_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_SOURCE_PATH)
        .expect("canonical scheduler source")
        .source;

    assert!(
        manifest.trusted_runtime_intrinsics.iter().any(|intrinsic| intrinsic.name == "guarded_stack_allocate"),
        "the manifest must own guarded stack allocation",
    );
    // `ConstantDefinition` binds a single `IntegerLiteral`, so the scheduler owns these
    // bounds as pre-folded literals rather than constant expressions.
    assert!(scheduler.contains("const FIBER_STACK_INITIAL_SIZE = 65536;"));
    assert!(scheduler.contains("const FIBER_STACK_MAX_SIZE = 8388608;"));
    assert!(scheduler.contains("pub pointer GuardedStackAllocate(word usableSize)"));
    assert!(scheduler.contains("return guarded_stack_allocate(usableSize);"));
    assert!(scheduler.contains("pointer stack = GuardedStackAllocate(FIBER_STACK_INITIAL_SIZE);"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 96), NativeWord(stack));"));
    assert!(scheduler.contains("SystemFree(context, ArchContextSize());"));
}
