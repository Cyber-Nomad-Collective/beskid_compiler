//! Parsed facts of the canonical scheduler runtime sources: the manifest-derived context
//! boundary, guarded fiber stacks, and the root-wait design for blocking primitives.
//!
//! Statement order and control flow are not checked here. Behaviour tests prove them:
//! the `Root*` tests in `runtime/beskid/tests/runtime_semantics/src/SchedulerTests.bd` (root
//! waits), `crates/beskid_engine/tests/native_kit/heap_growth_native.rs` (fiber stack limits and
//! `BESKID_FIBER_STACK_MAX`), and the `NativeNetwork*` tests in
//! `runtime/beskid/tests/runtime_semantics/src/NetworkNativeTests.bd` (root network waits).

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_CHANNEL_SOURCE_PATH, CANONICAL_FIBER_SOURCE_PATH, CANONICAL_HUB_SOURCE_PATH, CANONICAL_MUTEX_SOURCE_PATH,
    CANONICAL_PROCESS_SOURCE_PATH, CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH, CANONICAL_SCHEDULER_CORE_SOURCE_PATH,
    CANONICAL_SCHEDULER_LOOP_SOURCE_PATH, CANONICAL_SYSCALLS_SOURCE_PATH, CANONICAL_WAITGROUP_SOURCE_PATH,
    canonical_runtime_sources,
};
use beskid_tests_support::source_facts::SourceFacts;

/// `CANONICAL_SCHEDULER_EXTERNAL_SOURCE_PATH` is not re-exported by `runtime_source`.
const SCHEDULER_EXTERNAL_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/External.bd";

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

fn runtime_source(logical_path: &str) -> SourceFacts {
    let unit = canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == logical_path)
        .unwrap_or_else(|| panic!("canonical runtime source {logical_path} must exist"));
    SourceFacts::parse(logical_path, unit.source)
}

fn args(arguments: &[&str]) -> Vec<String> {
    arguments.iter().map(|argument| (*argument).to_owned()).collect()
}

fn assert_manifest_intrinsics(names: &[&str]) {
    let manifest = linux_manifest();
    for name in names {
        assert!(
            manifest.trusted_runtime_intrinsics.iter().any(|intrinsic| intrinsic.name == *name),
            "the canonical manifest must declare {name}"
        );
    }
}

#[test]
fn canonical_scheduler_context_boundary_is_manifest_derived() {
    assert_manifest_intrinsics(&["arch_context_size", "arch_context_alignment", "context_init", "context_switch"]);
    let context = runtime_source(CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH);
    for (function, signature, intrinsic, arguments) in [
        ("ArchContextSize", "pub word ArchContextSize()", "arch_context_size", args(&[])),
        ("ArchContextAlignment", "pub word ArchContextAlignment()", "arch_context_alignment", args(&[])),
        (
            "ContextInit",
            "pub unit ContextInit(pointer context, pointer stackTop, pointer entry, pointer argument, pointer returnTrampoline)",
            "context_init",
            args(&["context", "stackTop", "entry", "argument", "returnTrampoline"]),
        ),
        ("ContextSwitch", "pub unit ContextSwitch(pointer from, pointer to)", "context_switch", args(&["from", "to"])),
    ] {
        let facts = context.function(function);
        assert_eq!(facts.signature(), signature);
        assert_eq!(facts.call_arguments(intrinsic), [arguments], "{function} forwards to {intrinsic}");
    }

    let allocate = runtime_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    let allocate = allocate.function("FiberAlloc");
    assert!(
        allocate.call_arguments("SystemAllocate").contains(&args(&["ArchContextSize()", "ArchContextAlignment()"]))
    );
    let stores = allocate.call_arguments("raw_word_store");
    assert!(stores.contains(&args(&["pointer_add(fib,104)", "NativeWord(context)"])), "the record keeps the context");
    assert!(stores.contains(&args(&["pointer_add(fib,112)", "ArchContextSize()"])), "the record keeps its size");
}

#[test]
fn canonical_scheduler_uses_manifest_guarded_stacks_with_bounded_usable_storage() {
    assert_manifest_intrinsics(&["guarded_stack_allocate", "guarded_stack_grow", "guarded_stack_free"]);
    let context = runtime_source(CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH);
    for (function, signature, intrinsic, arguments) in [
        (
            "GuardedStackAllocate",
            "pub pointer GuardedStackAllocate(word initialSize, word maximumSize)",
            "guarded_stack_allocate",
            args(&["initialSize", "maximumSize"]),
        ),
        (
            "GuardedStackGrow",
            "pub bool GuardedStackGrow(pointer usableBase, word committedSize, word requestedSize, word maximumSize)",
            "guarded_stack_grow",
            args(&["usableBase", "committedSize", "requestedSize", "maximumSize"]),
        ),
        (
            "GuardedStackFree",
            "pub unit GuardedStackFree(pointer usableBase, word maximumSize)",
            "guarded_stack_free",
            args(&["usableBase", "maximumSize"]),
        ),
    ] {
        let facts = context.function(function);
        assert_eq!(facts.signature(), signature);
        assert_eq!(facts.call_arguments(intrinsic), [arguments], "{function} forwards to {intrinsic}");
    }

    let scheduler = runtime_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    assert_eq!(scheduler.constant("FIBER_STACK_INITIAL_SIZE"), 65536);
    assert_eq!(scheduler.constant("FIBER_STACK_MAX_SIZE"), 8388608);
    assert_eq!(scheduler.function("SchedulerFiberStackMax").signature(), "pub word SchedulerFiberStackMax()");
    assert_eq!(
        scheduler.function("SchedulerSetFiberStackMax").signature(),
        "pub bool SchedulerSetFiberStackMax(word maximumSize)"
    );

    // Every fiber reads the configured limit once; the same bound flows into the guarded
    // allocation, the record, the context top and the release.
    let allocate = scheduler.function("FiberAlloc");
    assert!(allocate.callees().contains("SchedulerFiberStackMax"));
    assert_eq!(
        allocate.call_arguments("GuardedStackAllocate"),
        [args(&["FIBER_STACK_INITIAL_SIZE", "stackMaximumSize"])]
    );
    let stores = allocate.call_arguments("raw_word_store");
    assert!(stores.contains(&args(&["pointer_add(fib,88)", "stackMaximumSize"])));
    assert!(stores.contains(&args(&["pointer_add(fib,120)", "FIBER_STACK_INITIAL_SIZE"])));
    let context_init = allocate.call_arguments("ContextInit");
    assert_eq!(context_init.len(), 1);
    assert_eq!(context_init[0][..2], args(&["context", "pointer_add(stack,stackMaximumSize)"]));
    assert!(allocate.call_arguments("GuardedStackFree").contains(&args(&["stack", "stackMaximumSize"])));
    assert!(allocate.call_arguments("SystemFree").contains(&args(&["context", "ArchContextSize()"])));

    let check = scheduler.function("SchedulerStackCheck");
    assert_eq!(check.signature(), "pub bool SchedulerStackCheck(word requiredUsableSize)");
    assert_eq!(
        check.call_arguments("GuardedStackGrow"),
        [args(&["stack", "committedSize", "requestedSize", "maximumSize"])]
    );
    let observed = scheduler.function("SchedulerStackOverflowObserved");
    assert_eq!(observed.signature(), "pub unit SchedulerStackOverflowObserved()");
    assert!(
        observed
            .call_arguments("raw_word_store")
            .contains(&args(&["pointer_add(fib,48)", "FIBER_JOIN_STACK_OVERFLOW"]))
    );
}

/// Main runs as the root fiber: every blocking primitive waits by driving the scheduler through
/// the one root-wait round instead of failing fast outside a fiber.
#[test]
fn blocking_primitives_wait_through_the_root_wait_round() {
    assert_eq!(
        runtime_source(CANONICAL_SCHEDULER_LOOP_SOURCE_PATH).function("SchedulerRootWait").signature(),
        "pub unit SchedulerRootWait()"
    );
    for path in [
        CANONICAL_CHANNEL_SOURCE_PATH,
        CANONICAL_WAITGROUP_SOURCE_PATH,
        CANONICAL_MUTEX_SOURCE_PATH,
        CANONICAL_HUB_SOURCE_PATH,
        CANONICAL_FIBER_SOURCE_PATH,
    ] {
        assert!(
            runtime_source(path).callees().contains("SchedulerRootWait"),
            "{path} must park the root caller through SchedulerRootWait"
        );
    }
}

/// External waits (timers, worker I/O, sockets, DNS) register the root caller under the root
/// waiter handle; only the parallel isolate and port waits register a fiber handle.
#[test]
fn external_waits_register_the_root_caller_as_a_waiter() {
    let external = runtime_source(SCHEDULER_EXTERNAL_SOURCE_PATH);
    assert_eq!(external.function("ExternalWaiterHandle").signature(), "pub word ExternalWaiterHandle()");
    assert_eq!(
        external.function("ExternalSleepUntil").call_arguments("ExternalWaitRegister"),
        [args(&["ExternalWaiterHandle()", "3", "deadline"])],
        "sleep registers the root"
    );
    assert!(runtime_source(CANONICAL_SYSCALLS_SOURCE_PATH).call_arguments("ExternalWaitRegister").contains(&args(&[
        "ExternalWaiterHandle()",
        "operation",
        "-1"
    ])));
    for unit in canonical_runtime_sources() {
        if unit.logical_path.contains("Parallel") {
            continue;
        }
        for arguments in SourceFacts::parse(&unit.logical_path, unit.source).call_arguments("ExternalWaitRegister") {
            assert_eq!(
                arguments.first().map(String::as_str),
                Some("ExternalWaiterHandle()"),
                "{} must register external waits through ExternalWaiterHandle",
                unit.logical_path
            );
        }
    }
}

/// A child pause in Main is a root sleep, so other fibers run while Main waits for its child.
#[test]
fn child_pause_in_main_is_a_root_sleep() {
    let pause = runtime_source(CANONICAL_PROCESS_SOURCE_PATH);
    assert!(pause.function("ChildPause").callees().contains("ExternalSleepUntil"));
}
