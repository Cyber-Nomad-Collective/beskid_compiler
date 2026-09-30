//! Workspace resolution reads `BESKID_CORELIB_ROOT` when materializing the implicit `Std`
//! dependency. Tests that set this variable must not run concurrently with any test that
//! resolves a project that relies on the default Std path.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

static STD_DEPENDENCY_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn std_dependency_env_lock() -> std::sync::MutexGuard<'static, ()> {
    STD_DEPENDENCY_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) struct ScopedStdDependencyRoot {
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

pub(crate) fn scoped_std_dependency_root(root: &Path) -> ScopedStdDependencyRoot {
    let guard = std_dependency_env_lock();
    let previous = std::env::var_os("BESKID_CORELIB_ROOT");
    // SAFETY: project tests that override the implicit Std root hold this process-wide lock.
    unsafe { std::env::set_var("BESKID_CORELIB_ROOT", root) };
    ScopedStdDependencyRoot { previous, _guard: guard }
}

impl Drop for ScopedStdDependencyRoot {
    fn drop(&mut self) {
        // SAFETY: the lock remains held until after Drop restores the previous value.
        if let Some(previous) = &self.previous {
            unsafe { std::env::set_var("BESKID_CORELIB_ROOT", previous) };
        } else {
            unsafe { std::env::remove_var("BESKID_CORELIB_ROOT") };
        }
    }
}
