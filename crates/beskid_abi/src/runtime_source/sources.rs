use crate::abi_v5::{SourceUnit, canonical_source_hash};

pub const CANONICAL_BOOTSTRAP_SOURCE_PATH: &str = "src/Runtime/Bootstrap.bd";
pub const CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH: &str = "src/Runtime/Bootstrap/Native.bd";
pub const CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH: &str = "src/Runtime/Bootstrap/Lifecycle.bd";
pub const CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH: &str = "src/Runtime/Bootstrap/Roots.bd";
pub const CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH: &str = "src/Runtime/Bootstrap/Objects.bd";
pub const CANONICAL_GC_SOURCE_PATH: &str = "src/Runtime/Mem/Gc.bd";
pub const CANONICAL_GC_STATE_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/State.bd";
pub const CANONICAL_GC_MARKING_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/Marking.bd";
pub const CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/RootsHandles.bd";
pub const CANONICAL_GC_SWEEP_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/Sweep.bd";
pub const CANONICAL_GC_COLLECTION_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/Collection.bd";
pub const CANONICAL_GC_ALLOCATION_SOURCE_PATH: &str = "src/Runtime/Mem/Gc/Allocation.bd";
pub const CANONICAL_STRINGS_SOURCE_PATH: &str = "src/Runtime/Data/Strings.bd";
pub const CANONICAL_COLLECTIONS_SOURCE_PATH: &str = "src/Runtime/Data/Collections.bd";
pub const CANONICAL_FIBER_SOURCE_PATH: &str = "src/Runtime/Fiber/Fiber.bd";
pub const CANONICAL_SCHEDULER_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler.bd";
pub const CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Context.bd";
pub const CANONICAL_SCHEDULER_CORE_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Core.bd";
pub const CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Storage.bd";
pub const CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Queue.bd";
pub const CANONICAL_SCHEDULER_LOOP_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Loop.bd";
pub const CANONICAL_SCHEDULER_POLL_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Poll.bd";
pub const CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH: &str = "src/Runtime/Fiber/Scheduler/Exports.bd";
pub const CANONICAL_CHANNEL_SOURCE_PATH: &str = "src/Runtime/Sync/Channel.bd";
pub const CANONICAL_MUTEX_SOURCE_PATH: &str = "src/Runtime/Sync/Mutex.bd";
pub const CANONICAL_WAITGROUP_SOURCE_PATH: &str = "src/Runtime/Sync/WaitGroup.bd";
pub const CANONICAL_HUB_SOURCE_PATH: &str = "src/Runtime/PubSub/Hub.bd";
pub const CANONICAL_EVENTS_SOURCE_PATH: &str = "src/Runtime/PubSub/Events.bd";
pub const CANONICAL_DYNAMIC_SOURCE_PATH: &str = "src/Runtime/Dynamic/Dynamic.bd";
pub const CANONICAL_CLOCKS_SOURCE_PATH: &str = "src/Runtime/Host/Clocks.bd";
pub const CANONICAL_PROCESS_SOURCE_PATH: &str = "src/Runtime/Host/Process.bd";
pub const CANONICAL_FS_SOURCE_PATH: &str = "src/Runtime/Host/FS.bd";
pub const CANONICAL_COMPOSITION_SOURCE_PATH: &str = "src/Runtime/Host/Composition.bd";
pub const CANONICAL_CALLBACKS_SOURCE_PATH: &str = "src/Runtime/Host/Callbacks.bd";
pub const CANONICAL_SYSCALLS_SOURCE_PATH: &str = "src/Runtime/Io/Syscalls.bd";

/// Canonical Foundation syscall facade eligible for Corelib service authority.
pub const CANONICAL_CORELIB_SYSCALL_SOURCE_PATH: &str = "Core/Syscall/Syscall.bd";
/// Canonical Foundation process-argument facade eligible for its two private ABI-v5 services.
pub const CANONICAL_CORELIB_ARGS_SOURCE_PATH: &str = "Core/Args/Args.bd";
/// Canonical Foundation filesystem facade eligible for private ABI-v5 filesystem services.
pub const CANONICAL_CORELIB_FS_SOURCE_PATH: &str = "Core/FS/FS.bd";
/// Canonical Foundation assertion helper eligible to import the panic runtime service.
pub const CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH: &str = "Testing/Assert.bd";
/// Canonical Foundation output helper eligible to import the panic runtime service.
pub const CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH: &str = "Core/Output/Output.bd";
/// Canonical Foundation error helper eligible to import the panic runtime service.
pub const CANONICAL_FOUNDATION_ERROR_SOURCE_PATH: &str = "Core/Error/Error.bd";

const CANONICAL_BOOTSTRAP_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Bootstrap.bd"));
const CANONICAL_BOOTSTRAP_NATIVE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Bootstrap/Native.bd"));
const CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Bootstrap/Lifecycle.bd"));
const CANONICAL_BOOTSTRAP_ROOTS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Bootstrap/Roots.bd"));
const CANONICAL_BOOTSTRAP_OBJECTS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Bootstrap/Objects.bd"));
const CANONICAL_GC_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc.bd"));
const CANONICAL_GC_STATE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/State.bd"));
const CANONICAL_GC_MARKING_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/Marking.bd"));
const CANONICAL_GC_ROOTS_HANDLES_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/RootsHandles.bd"));
const CANONICAL_GC_SWEEP_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/Sweep.bd"));
const CANONICAL_GC_COLLECTION_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/Collection.bd"));
const CANONICAL_GC_ALLOCATION_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Mem/Gc/Allocation.bd"));
const CANONICAL_STRINGS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Data/Strings.bd"));
const CANONICAL_COLLECTIONS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Data/Collections.bd"));
const CANONICAL_FIBER_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Fiber.bd"));
const CANONICAL_SCHEDULER_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler.bd"));
const CANONICAL_SCHEDULER_CONTEXT_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Context.bd"));
const CANONICAL_SCHEDULER_CORE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Core.bd"));
const CANONICAL_SCHEDULER_STORAGE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Storage.bd"));
const CANONICAL_SCHEDULER_QUEUE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Queue.bd"));
const CANONICAL_SCHEDULER_LOOP_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Loop.bd"));
const CANONICAL_SCHEDULER_POLL_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Poll.bd"));
const CANONICAL_SCHEDULER_EXPORTS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Fiber/Scheduler/Exports.bd"));
const CANONICAL_CHANNEL_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Sync/Channel.bd"));
const CANONICAL_MUTEX_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Sync/Mutex.bd"));
const CANONICAL_WAITGROUP_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Sync/WaitGroup.bd"));
const CANONICAL_HUB_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/PubSub/Hub.bd"));
const CANONICAL_EVENTS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/PubSub/Events.bd"));
const CANONICAL_DYNAMIC_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Dynamic/Dynamic.bd"));
const CANONICAL_CLOCKS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Host/Clocks.bd"));
const CANONICAL_PROCESS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Host/Process.bd"));
const CANONICAL_FS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Host/FS.bd"));
const CANONICAL_COMPOSITION_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Host/Composition.bd"));
const CANONICAL_CALLBACKS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Host/Callbacks.bd"));
const CANONICAL_SYSCALLS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/beskid/src/Runtime/Io/Syscalls.bd"));

const CANONICAL_CORELIB_SYSCALL_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Syscall/Syscall.bd"));
const CANONICAL_CORELIB_ARGS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Args/Args.bd"));
const CANONICAL_CORELIB_FS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/FS/FS.bd"));
const CANONICAL_FOUNDATION_ASSERT_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Testing/Assert.bd"));
const CANONICAL_FOUNDATION_OUTPUT_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Output/Output.bd"));
const CANONICAL_FOUNDATION_ERROR_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Error/Error.bd"));

/// The runtime source corpus built into this compiler version.
pub fn canonical_runtime_sources() -> Vec<SourceUnit> {
    vec![
        SourceUnit { logical_path: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(), source: CANONICAL_BOOTSTRAP_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH.into(),
            source: CANONICAL_BOOTSTRAP_NATIVE_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH.into(),
            source: CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH.into(),
            source: CANONICAL_BOOTSTRAP_ROOTS_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH.into(),
            source: CANONICAL_BOOTSTRAP_OBJECTS_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_GC_SOURCE_PATH.into(), source: CANONICAL_GC_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_GC_STATE_SOURCE_PATH.into(), source: CANONICAL_GC_STATE_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_GC_MARKING_SOURCE_PATH.into(),
            source: CANONICAL_GC_MARKING_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH.into(),
            source: CANONICAL_GC_ROOTS_HANDLES_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_GC_SWEEP_SOURCE_PATH.into(), source: CANONICAL_GC_SWEEP_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_GC_COLLECTION_SOURCE_PATH.into(),
            source: CANONICAL_GC_COLLECTION_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_GC_ALLOCATION_SOURCE_PATH.into(),
            source: CANONICAL_GC_ALLOCATION_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_STRINGS_SOURCE_PATH.into(), source: CANONICAL_STRINGS_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_COLLECTIONS_SOURCE_PATH.into(),
            source: CANONICAL_COLLECTIONS_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_FIBER_SOURCE_PATH.into(), source: CANONICAL_FIBER_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_SCHEDULER_SOURCE_PATH.into(), source: CANONICAL_SCHEDULER_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_CONTEXT_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_CORE_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_CORE_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_STORAGE_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_QUEUE_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_LOOP_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_LOOP_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_POLL_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_POLL_SOURCE.into(),
        },
        SourceUnit {
            logical_path: CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH.into(),
            source: CANONICAL_SCHEDULER_EXPORTS_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_CHANNEL_SOURCE_PATH.into(), source: CANONICAL_CHANNEL_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_MUTEX_SOURCE_PATH.into(), source: CANONICAL_MUTEX_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_WAITGROUP_SOURCE_PATH.into(), source: CANONICAL_WAITGROUP_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_HUB_SOURCE_PATH.into(), source: CANONICAL_HUB_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_EVENTS_SOURCE_PATH.into(), source: CANONICAL_EVENTS_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_DYNAMIC_SOURCE_PATH.into(), source: CANONICAL_DYNAMIC_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_CLOCKS_SOURCE_PATH.into(), source: CANONICAL_CLOCKS_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_PROCESS_SOURCE_PATH.into(), source: CANONICAL_PROCESS_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_FS_SOURCE_PATH.into(), source: CANONICAL_FS_SOURCE.into() },
        SourceUnit {
            logical_path: CANONICAL_COMPOSITION_SOURCE_PATH.into(),
            source: CANONICAL_COMPOSITION_SOURCE.into(),
        },
        SourceUnit { logical_path: CANONICAL_CALLBACKS_SOURCE_PATH.into(), source: CANONICAL_CALLBACKS_SOURCE.into() },
        SourceUnit { logical_path: CANONICAL_SYSCALLS_SOURCE_PATH.into(), source: CANONICAL_SYSCALLS_SOURCE.into() },
    ]
}

/// The compiler-embedded Corelib syscall facade. This is deliberately a distinct source corpus
/// from the runtime bootstrap: Corelib services must never borrow runtime-intrinsic authority.
pub fn canonical_corelib_syscall_sources() -> Vec<SourceUnit> {
    vec![SourceUnit {
        logical_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_SYSCALL_SOURCE.into(),
    }]
}

/// Compiler-embedded Foundation units eligible for distinct ABI service authority.
pub fn canonical_corelib_service_sources() -> Vec<SourceUnit> {
    let mut sources = canonical_corelib_syscall_sources();
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_ARGS_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_ARGS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_FS_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_FS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_ASSERT_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_OUTPUT_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_ERROR_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_ERROR_SOURCE.into(),
    });
    sources
}

/// Hash of the corpus embedded in this compiler and eligible for ABI-v5 runtime authority.
pub fn canonical_runtime_source_hash() -> String {
    canonical_source_hash(&canonical_runtime_sources()).expect("embedded runtime source paths are unique")
}
