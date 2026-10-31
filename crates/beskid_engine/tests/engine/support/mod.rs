//! Test support shared by every engine suite of this binary.

pub mod explicit_kit;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
pub mod native_fixture;
pub mod shared_kit;

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Exclusive use of the runtime kit that the environment names (`BESKID_RUNTIME_PREFIX`, as
/// `Engine::new` and the staged-kit proofs load it).
///
/// Two engines that load the same kit at the same time cannot both initialize its runtime, so a
/// test holds this lease for as long as such an engine lives.
pub fn environment_kit_lease() -> MutexGuard<'static, ()> {
    static EXCLUSIVE: Mutex<()> = Mutex::new(());
    EXCLUSIVE.lock().unwrap_or_else(PoisonError::into_inner)
}
