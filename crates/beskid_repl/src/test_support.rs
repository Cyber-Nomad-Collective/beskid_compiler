//! Shared fixtures for the REPL unit tests.
//!
//! Two pieces of process-wide state make REPL cases unsafe to run concurrently in one libtest
//! binary:
//!
//! - every snippet is prepared under the fixed [`crate::REPL_SOURCE_PATH`], so concurrent cases
//!   share one entry-session fingerprint in the analysis registry and replace each other's syntax
//!   generation between prepare and the executable cache store;
//! - an [`beskid_engine::Engine`] activates the loaded runtime image's scheduler and worker pool,
//!   which exist once per loaded kit image, so two engines over one kit cannot be active at once.
//!
//! A production REPL owns one session on one thread, so neither case arises outside tests.
//! [`serial`] restores that model for each case; it also covers `BESKID_RUNTIME_PREFIX` mutation.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

static REPL_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Hold for the whole case that prepares a snippet, builds an engine, or reads the runtime env.
pub(crate) fn serial() -> MutexGuard<'static, ()> {
    REPL_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One exact debug kit published for this test binary.
pub(crate) fn shared_exact_kit_prefix() -> &'static Path {
    static PREFIX: OnceLock<PathBuf> = OnceLock::new();
    PREFIX.get_or_init(|| {
        let prefix = tempfile::tempdir().expect("exact kit prefix").keep();
        build_native_host(prefix.clone(), RuntimeKitProfile::Debug).expect("publish exact native kit");
        prefix
    })
}

/// Engine over [`shared_exact_kit_prefix`]; call only while holding [`serial`].
pub(crate) fn exact_kit_engine() -> Engine {
    let target = host_runtime_target().expect("supported native host target");
    Engine::with_runtime_kit(shared_exact_kit_prefix(), target, BuildProfile::Debug).expect("load exact kit")
}
