//! Location of the compiler workspace for project tests.

use std::path::PathBuf;

/// `compiler/crates/beskid_tests_projects` → compiler workspace root (`compiler/`), where `corelib/beskid_corelib` exists.
pub(crate) fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("beskid_tests_projects must live at compiler/crates/beskid_tests_projects")
        .to_path_buf()
}
