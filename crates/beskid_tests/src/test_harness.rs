//! Small filesystem helpers shared by project-resolution and workspace tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::projects::{PROJECT_FILE_NAME, WORKSPACE_FILE_NAME};

/// Unique temp directory under the OS temp dir (PID + time); caller should remove when done.
pub(crate) fn temp_case_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_tests_{prefix}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Write a `Project.bd` manifest and return its path.
pub(crate) fn write_project_manifest(dir: impl AsRef<Path>, source: &str) -> PathBuf {
    let manifest_path = dir.as_ref().join(PROJECT_FILE_NAME);
    fs::write(&manifest_path, source).expect("write project manifest");
    manifest_path
}

/// Write a `Workspace.bd` manifest and return its path.
pub(crate) fn write_workspace_manifest(dir: impl AsRef<Path>, source: &str) -> PathBuf {
    let manifest_path = dir.as_ref().join(WORKSPACE_FILE_NAME);
    fs::write(&manifest_path, source).expect("write workspace manifest");
    manifest_path
}

/// Assert two paths refer to the same file after `canonicalize` (cross-symlink comparisons).
#[track_caller]
pub(crate) fn assert_same_canonical_path(left: impl AsRef<Path>, right: impl AsRef<Path>) {
    assert_eq!(
        left.as_ref()
            .canonicalize()
            .expect("left path canonicalize"),
        right
            .as_ref()
            .canonicalize()
            .expect("right path canonicalize"),
    );
}
