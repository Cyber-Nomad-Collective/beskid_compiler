//! Process-global `current_dir` guard for project tests that trigger corelib discovery.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `discover_repo_corelib_root` uses `std::env::current_dir()`; keep it inside the temp workspace
/// so tests do not pick up a polluted `compiler/` tree. Serialized because `set_current_dir` is process-global.
pub(crate) static PROJECT_TEST_CWD_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn with_cwd_at_workspace_root<R>(root: &Path, f: impl FnOnce() -> R) -> R {
    let _guard = PROJECT_TEST_CWD_LOCK.lock().expect("project test cwd lock");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(root).expect("chdir to temp workspace");
    let out = f();
    std::env::set_current_dir(previous).expect("restore cwd");
    out
}

/// `compiler/crates/beskid_tests` → compiler workspace root (`compiler/`), where `corelib/beskid_corelib` exists for implicit `Std`.
pub(crate) fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("beskid_tests must live at compiler/crates/beskid_tests")
        .to_path_buf()
}
