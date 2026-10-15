//! Workspace resolution reads `BESKID_CORELIB_ROOT` when materializing the implicit `Core`
//! dependency. Tests that set this variable must not run concurrently with any test that
//! resolves a project that relies on the default Core path.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

static CORE_DEPENDENCY_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn core_dependency_env_lock() -> std::sync::MutexGuard<'static, ()> {
    CORE_DEPENDENCY_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) struct ScopedCoreDependencyRoot {
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

pub(crate) fn scoped_core_dependency_root(root: &Path) -> ScopedCoreDependencyRoot {
    let guard = core_dependency_env_lock();
    let previous = std::env::var_os("BESKID_CORELIB_ROOT");
    // SAFETY: project tests that override the implicit Core root hold this process-wide lock.
    unsafe { std::env::set_var("BESKID_CORELIB_ROOT", root) };
    ScopedCoreDependencyRoot { previous, _guard: guard }
}

impl Drop for ScopedCoreDependencyRoot {
    fn drop(&mut self) {
        // SAFETY: the lock remains held until after Drop restores the previous value.
        if let Some(previous) = &self.previous {
            unsafe { std::env::set_var("BESKID_CORELIB_ROOT", previous) };
        } else {
            unsafe { std::env::remove_var("BESKID_CORELIB_ROOT") };
        }
    }
}
