//! Process-global cwd guard for integration tests that call `set_current_dir`.

use std::path::Path;
use std::sync::Mutex;

/// Serialized because `set_current_dir` is process-global (parallel tests must not interleave cwd).
static PROJECT_TEST_CWD_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with cwd set to `dir`, restoring the previous cwd afterward.
pub(crate) fn with_cwd<R>(dir: &Path, f: impl FnOnce() -> R) -> R {
    let _guard = PROJECT_TEST_CWD_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir).expect("chdir");
    let out = f();
    std::env::set_current_dir(previous).expect("restore cwd");
    out
}
