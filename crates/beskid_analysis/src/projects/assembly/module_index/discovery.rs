use std::path::{Path, PathBuf};

pub(super) fn normalize_assembly_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
