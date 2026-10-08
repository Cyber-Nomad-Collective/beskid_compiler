//! One canonical native runtime kit per test process.
//!
//! `build_native_host` lowers, links and publishes the embedded ABI-v5 runtime corpus. That takes
//! tens of seconds, and the corpus is fixed for the life of the process, so a second build in the
//! same test binary produces the same kit. Tests that only consume a kit share this one; tests
//! that exercise kit construction or alter kit files keep calling `build_native_host` with their
//! own prefix. Consumers must treat the shared prefix as read-only and put scratch files in their
//! own temporary directory.

// Each test binary includes this module and uses a different part of it.
#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use beskid_abi::runtime_kit::ResolvedRuntimeKit;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

pub struct SharedKit {
    prefix: PathBuf,
    pub kit: ResolvedRuntimeKit,
}

impl SharedKit {
    /// Prefix to pass to `Engine::with_runtime_kit` or `BESKID_RUNTIME_PREFIX`.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }
}

/// The debug-profile canonical kit for this process, built on first use.
pub fn debug() -> &'static SharedKit {
    static KIT: OnceLock<SharedKit> = OnceLock::new();
    KIT.get_or_init(|| {
        // The kit outlives every test, so keep it under Cargo's per-target scratch directory
        // (removed by `cargo clean`) rather than leaking it into the system temporary directory.
        let prefix = tempfile::Builder::new()
            .prefix("shared-native-kit-")
            .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
            .expect("shared runtime-kit prefix")
            .keep();
        let kit = build_native_host(prefix.clone(), RuntimeKitProfile::Debug).expect("publish shared native kit");
        SharedKit { prefix, kit }
    })
}
