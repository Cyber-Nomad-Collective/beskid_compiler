use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_source::{
        CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH, CANONICAL_SCHEDULER_CORE_SOURCE_PATH, canonical_runtime_sources,
    },
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
    let sources = canonical_runtime_sources();
    let context = sources
        .iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH)
        .expect("canonical scheduler context source");
    let scheduler = &sources
        .iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_CORE_SOURCE_PATH)
        .expect("canonical scheduler core source")
        .source;

    for name in ["arch_context_size", "arch_context_alignment", "context_init", "context_switch"] {
        assert!(
            manifest.trusted_runtime_intrinsics.iter().any(|intrinsic| intrinsic.name == name),
            "canonical manifest must declare {name}",
        );
    }
    assert!(context.source.contains("pub word ArchContextSize()"));
    assert!(context.source.contains("return arch_context_size();"));
    assert!(context.source.contains("pub word ArchContextAlignment()"));
    assert!(context.source.contains("return arch_context_alignment();"));
    assert!(context.source.contains("pub unit ContextInit("));
    assert!(context.source.contains("context_init(context, stackTop, entry, argument, returnTrampoline);"));
    assert!(context.source.contains("pub unit ContextSwitch(pointer from, pointer to)"));
    assert!(context.source.contains("context_switch(from, to);"));
    assert!(scheduler.contains("pointer context = SystemAllocate(ArchContextSize(), ArchContextAlignment());"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 104), NativeWord(context));"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 112), ArchContextSize());"));
}

#[test]
fn canonical_scheduler_uses_manifest_guarded_stacks_with_bounded_usable_storage() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let context = sources
        .iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH)
        .expect("canonical scheduler context source");
    let scheduler = &sources
        .iter()
        .find(|source| source.logical_path == CANONICAL_SCHEDULER_CORE_SOURCE_PATH)
        .expect("canonical scheduler core source")
        .source;

    for name in ["guarded_stack_allocate", "guarded_stack_grow", "guarded_stack_free"] {
        assert!(
            manifest.trusted_runtime_intrinsics.iter().any(|intrinsic| intrinsic.name == name),
            "the manifest must own {name}",
        );
    }
    // `ConstantDefinition` binds a single `IntegerLiteral`, so the scheduler owns these
    // bounds as pre-folded literals rather than constant expressions.
    assert!(scheduler.contains("const FIBER_STACK_INITIAL_SIZE = 65536;"));
    assert!(scheduler.contains("const FIBER_STACK_MAX_SIZE = 8388608;"));
    assert!(context.source.contains("pub pointer GuardedStackAllocate(word initialSize, word maximumSize)"));
    assert!(context.source.contains("return guarded_stack_allocate(initialSize, maximumSize);"));
    assert!(context.source.contains("pub bool GuardedStackGrow("));
    // The per-fiber reservation is the configured scheduler limit (`BESKID_FIBER_STACK_MAX`,
    // `SchedulerSetFiberStackMax`), defaulting to `FIBER_STACK_MAX_SIZE`; every fiber reads it once
    // and the same bound flows into the guarded allocation, the record and the context top.
    assert!(scheduler.contains("pub word SchedulerFiberStackMax()"));
    assert!(scheduler.contains("if table == NativePointer(0) { return FIBER_STACK_MAX_SIZE; }"));
    assert!(scheduler.contains("if configured == 0 { return FIBER_STACK_MAX_SIZE; }"));
    assert!(scheduler.contains("pub bool SchedulerSetFiberStackMax(word maximumSize)"));
    assert!(scheduler.contains(
        "if maximumSize < FIBER_STACK_INITIAL_SIZE || maximumSize % FIBER_STACK_INITIAL_SIZE != 0 { return false; }"
    ));
    assert!(scheduler.contains("word stackMaximumSize = SchedulerFiberStackMax();"));
    assert!(scheduler.contains("pointer stack = GuardedStackAllocate(FIBER_STACK_INITIAL_SIZE, stackMaximumSize);"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 88), stackMaximumSize);"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 120), FIBER_STACK_INITIAL_SIZE);"));
    assert!(scheduler.contains("ContextInit(context, pointer_add(stack, stackMaximumSize)"));
    assert!(scheduler.contains("GuardedStackFree(stack, stackMaximumSize);"));
    assert!(scheduler.contains("pub bool SchedulerStackCheck(word requiredUsableSize)"));
    assert!(scheduler.contains("if requiredUsableSize <= committedSize { return true; }"));
    assert!(scheduler.contains("if requiredUsableSize > maximumSize"));
    assert!(scheduler.contains("GuardedStackGrow(stack, committedSize, requestedSize, maximumSize)"));
    assert!(scheduler.contains("pub unit SchedulerStackOverflowObserved()"));
    assert!(scheduler.contains("raw_word_store(pointer_add(fib, 48), FIBER_JOIN_STACK_OVERFLOW);"));
    assert!(scheduler.contains("SystemFree(context, ArchContextSize());"));
}
