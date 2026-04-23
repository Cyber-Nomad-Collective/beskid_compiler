//! Workspace resolution reads `BESKID_CORELIB_ROOT` when materializing the implicit `Std`
//! dependency. Tests that set this variable must not run concurrently with any test that
//! resolves a project that relies on the default Std path.

use std::sync::{Mutex, OnceLock};

static STD_DEPENDENCY_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn std_dependency_env_lock() -> std::sync::MutexGuard<'static, ()> {
    STD_DEPENDENCY_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("std dependency env lock poisoned")
}
