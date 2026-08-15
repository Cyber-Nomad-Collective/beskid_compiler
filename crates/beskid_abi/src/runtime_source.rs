//! Compiler-embedded authority for canonical Beskid runtime sources.
//!
//! This module grants no ambient or serializable package capability. The frontend may use the
//! returned token only for AST nodes from the exact embedded source corpus.

mod capabilities;
mod corelib_services;
mod kits;
mod sources;

pub use capabilities::{
    CanonicalRuntimeProof, RuntimeCapabilityError, RuntimeIntrinsicCapability, canonical_runtime_intrinsic_capability,
    prove_canonical_runtime_corpus,
};
pub use corelib_services::{
    CorelibService, CorelibServiceCapability, CorelibServiceProof, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_corelib_syscall_service_capability,
};
pub use kits::{
    CanonicalRuntimeKitBuildError, CanonicalRuntimeKitError, build_canonical_runtime_kit, resolve_canonical_runtime_kit,
};
pub use sources::{
    CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
    CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_SOURCE_PATH,
    CANONICAL_CALLBACKS_SOURCE_PATH, CANONICAL_CHANNEL_SOURCE_PATH, CANONICAL_CLOCKS_SOURCE_PATH,
    CANONICAL_COLLECTIONS_SOURCE_PATH, CANONICAL_COMPOSITION_SOURCE_PATH, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_DYNAMIC_SOURCE_PATH, CANONICAL_EVENTS_SOURCE_PATH,
    CANONICAL_FIBER_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, CANONICAL_FOUNDATION_ERROR_SOURCE_PATH,
    CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH, CANONICAL_GC_ALLOCATION_SOURCE_PATH, CANONICAL_GC_COLLECTION_SOURCE_PATH,
    CANONICAL_GC_MARKING_SOURCE_PATH, CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH, CANONICAL_GC_SOURCE_PATH,
    CANONICAL_GC_STATE_SOURCE_PATH, CANONICAL_GC_SWEEP_SOURCE_PATH, CANONICAL_HUB_SOURCE_PATH,
    CANONICAL_MUTEX_SOURCE_PATH, CANONICAL_PROCESS_SOURCE_PATH, CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH,
    CANONICAL_SCHEDULER_LOOP_SOURCE_PATH, CANONICAL_SCHEDULER_POLL_SOURCE_PATH, CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH,
    CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH, CANONICAL_STRINGS_SOURCE_PATH, CANONICAL_SYSCALLS_SOURCE_PATH,
    CANONICAL_WAITGROUP_SOURCE_PATH, canonical_corelib_service_sources, canonical_corelib_syscall_sources,
    canonical_runtime_source_hash, canonical_runtime_sources,
};

#[cfg(test)]
mod tests;
