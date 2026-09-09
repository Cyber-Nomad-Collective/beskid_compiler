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
/// Canonical concurrency scheduler facade eligible for its private ABI-v5 services.
pub const CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH: &str = "Concurrency.bd";
/// Canonical concurrency Fiber facade eligible for lifecycle service authority.
pub const CANONICAL_CORELIB_FIBER_SOURCE_PATH: &str = "Concurrency/Fiber.bd";
/// Canonical Console Linux platform facade eligible for terminal service authority.
pub const CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH: &str = "Platform/Linux.bd";
/// Canonical Console macOS platform facade eligible for terminal service authority.
pub const CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH: &str = "Platform/MacOS.bd";
/// Canonical Console Windows platform facade eligible for terminal service authority.
pub const CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH: &str = "Platform/Windows.bd";
/// Canonical Console terminal facade eligible for environment lookup authority.
pub const CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH: &str = "Platform/Terminal.bd";
/// Canonical Foundation filesystem facade eligible for private ABI-v5 filesystem services.
pub const CANONICAL_CORELIB_FS_SOURCE_PATH: &str = "Core/FS/FS.bd";
/// Canonical concurrency channel facade eligible for its private ABI-v5 services.
pub const CANONICAL_CORELIB_CHANNEL_SOURCE_PATH: &str = "Concurrency/Channel.bd";
/// Canonical concurrency mutex facade eligible for its private ABI-v5 services.
pub const CANONICAL_CORELIB_MUTEX_SOURCE_PATH: &str = "Concurrency/Mutex.bd";
/// Canonical concurrency hub facade eligible for its private ABI-v5 services.
pub const CANONICAL_CORELIB_HUB_SOURCE_PATH: &str = "Concurrency/Hub.bd";
/// Canonical concurrency wait-group facade eligible for its private ABI-v5 services.
pub const CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH: &str = "Concurrency/WaitGroup.bd";
/// Canonical Foundation array facade eligible for compiler-owned typed allocation lowering.
pub const CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH: &str = "Core/Collections/Array.bd";
/// Canonical Foundation environment facade eligible for host environment runtime services.
pub const CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH: &str = "Core/Environment/Environment.bd";
/// Canonical Foundation path facade eligible for string-slice runtime services.
pub const CANONICAL_FOUNDATION_PATH_SOURCE_PATH: &str = "Core/Path/Path.bd";
/// Canonical Foundation process facade eligible for the implemented process runtime services.
pub const CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH: &str = "Core/Process/Process.bd";
/// Canonical Foundation random facade eligible for the monotonic-clock runtime service.
pub const CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH: &str = "Core/Random/Random.bd";
/// Canonical Foundation core string facade eligible for string runtime services.
pub const CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH: &str = "Core/String/Core.bd";
/// Canonical Foundation UTF-8 facade eligible for byte-array and string runtime services.
pub const CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH: &str = "Core/String/Utf8.bd";
/// Canonical Foundation text cursor eligible for string-slice runtime services.
pub const CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH: &str = "Core/Text/Cursor.bd";
/// Canonical Foundation time facade eligible for realtime and monotonic clock services.
pub const CANONICAL_FOUNDATION_TIME_SOURCE_PATH: &str = "Core/Time/Time.bd";
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
const CANONICAL_CORELIB_CONCURRENCY_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/concurrency/src/Concurrency.bd"));
const CANONICAL_CORELIB_FIBER_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/concurrency/src/Concurrency/Fiber.bd"));
const CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/console/src/Platform/Linux.bd"));
const CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/console/src/Platform/MacOS.bd"));
const CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/console/src/Platform/Windows.bd"));
const CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/console/src/Platform/Terminal.bd"));
const CANONICAL_CORELIB_FS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/FS/FS.bd"));
const CANONICAL_CORELIB_CHANNEL_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/concurrency/src/Concurrency/Channel.bd"));
const CANONICAL_CORELIB_MUTEX_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/concurrency/src/Concurrency/Mutex.bd"));
const CANONICAL_CORELIB_HUB_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/concurrency/src/Concurrency/Hub.bd"));
const CANONICAL_CORELIB_WAIT_GROUP_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/concurrency/src/Concurrency/WaitGroup.bd"
));
const CANONICAL_FOUNDATION_ARRAY_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/foundation/src/Core/Collections/Array.bd"
));
const CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/foundation/src/Core/Environment/Environment.bd"
));
const CANONICAL_FOUNDATION_PATH_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Path/Path.bd"));
const CANONICAL_FOUNDATION_PROCESS_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Process/Process.bd"));
const CANONICAL_FOUNDATION_RANDOM_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Random/Random.bd"));
const CANONICAL_FOUNDATION_STRING_CORE_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/String/Core.bd"));
const CANONICAL_FOUNDATION_STRING_UTF8_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/String/Utf8.bd"));
const CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Text/Cursor.bd"));
const CANONICAL_FOUNDATION_TIME_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Time/Time.bd"));
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
        logical_path: CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CONCURRENCY_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_FIBER_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_FS_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_FS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_CHANNEL_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_MUTEX_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_MUTEX_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_HUB_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_HUB_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_WAIT_GROUP_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_ARRAY_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_PATH_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_PATH_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_PROCESS_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_RANDOM_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_STRING_CORE_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_STRING_UTF8_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_TIME_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_TIME_SOURCE.into(),
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
