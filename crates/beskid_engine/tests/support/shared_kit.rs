//! One canonical native runtime kit per test process.
//!
//! `build_native_host` lowers, links and publishes the embedded ABI-v5 runtime corpus. That takes
//! tens of seconds, and the corpus is fixed for the life of the process, so a second build in the
//! same test binary produces the same kit. Tests that only consume a kit share this one; tests
//! that exercise kit construction or alter kit files keep calling `build_native_host` with their
//! own prefix. Consumers must treat the shared prefix as read-only and put scratch files in their
//! own temporary directory.
//!
//! The runtime keeps its process state in the loaded runtime library, so two engines that load
//! the same kit at the same time cannot both initialize it. [`debug`] therefore hands out an
//! exclusive lease; a test holds it for as long as it uses the kit, which serializes kit
//! consumers in one binary even under the parallel test harness.

// Each test binary includes this module and uses a different part of it.
#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

use beskid_abi::runtime_kit::ResolvedRuntimeKit;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

struct SharedKit {
    prefix: PathBuf,
    kit: ResolvedRuntimeKit,
}

/// Exclusive use of the shared kit; release it by dropping it when the test no longer runs code
/// against the kit.
pub struct SharedKitLease {
    shared: &'static SharedKit,
    _exclusive: MutexGuard<'static, ()>,
}

impl SharedKitLease {
    /// Prefix to pass to `Engine::with_runtime_kit` or `BESKID_RUNTIME_PREFIX`.
    pub fn prefix(&self) -> &Path {
        &self.shared.prefix
    }

    pub fn kit(&self) -> &ResolvedRuntimeKit {
        &self.shared.kit
    }
}

/// Lease the debug-profile canonical kit for this process, building it on first use.
pub fn debug() -> SharedKitLease {
    static EXCLUSIVE: Mutex<()> = Mutex::new(());
    static KIT: OnceLock<SharedKit> = OnceLock::new();
    // A test that panicked while holding the lease left no runtime state behind: its engine was
    // dropped during unwinding.
    let exclusive = EXCLUSIVE.lock().unwrap_or_else(PoisonError::into_inner);
    let shared = KIT.get_or_init(|| {
        // The kit outlives every test, so keep it under Cargo's per-target scratch directory
        // (removed by `cargo clean`) rather than leaking it into the system temporary directory.
        let prefix = tempfile::Builder::new()
            .prefix("shared-native-kit-")
            .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
            .expect("shared runtime-kit prefix")
            .keep();
        let kit = build_native_host(prefix.clone(), RuntimeKitProfile::Debug).expect("publish shared native kit");
        SharedKit { prefix, kit }
    });
    SharedKitLease { shared, _exclusive: exclusive }
}
