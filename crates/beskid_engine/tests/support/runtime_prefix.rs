use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Scoped exact-kit prefix for an integration-test process.
///
/// `BESKID_RUNTIME_PREFIX` is process-global, while libtest runs cases in one binary
/// concurrently. This guard holds the test-binary lock from the mutation through the
/// production lookup and restores the previous value before another case can proceed.
static RUNTIME_PREFIX_LOCK: Mutex<()> = Mutex::new(());

pub struct RuntimePrefixContext {
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl RuntimePrefixContext {
    pub fn install(prefix: &Path) -> Self {
        let lock = RUNTIME_PREFIX_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("BESKID_RUNTIME_PREFIX");
        // SAFETY: the process-wide lock prevents concurrent reads or writes in this integration
        // target, and Drop restores the exact pre-test value before releasing that lock.
        unsafe { std::env::set_var("BESKID_RUNTIME_PREFIX", prefix) };
        Self { previous, _lock: lock }
    }
}

impl Drop for RuntimePrefixContext {
    fn drop(&mut self) {
        // SAFETY: `RuntimePrefixContext::install` holds the process-wide lock for this mutation.
        unsafe {
            if let Some(value) = &self.previous {
                std::env::set_var("BESKID_RUNTIME_PREFIX", value);
            } else {
                std::env::remove_var("BESKID_RUNTIME_PREFIX");
            }
        }
    }
}
