//! Canonical path comparison for scoped resolve/type/codegen tables.

use std::path::{Path, PathBuf};

/// Stable key for per-unit scoped tables (canonical when the path exists on disk).
pub fn unit_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Whether two paths refer to the same on-disk file (equality or matching canonical paths).
pub fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(a), Ok(b)) if a == b => return true,
        _ => {}
    }
    logical_source_suffix(left)
        .is_some_and(|left_suffix| logical_source_suffix(right).is_some_and(|right_suffix| left_suffix == right_suffix))
}

/// Suffix from the final `/src/` segment (stable across materialized `obj/` copies).
fn logical_source_suffix(path: &Path) -> Option<PathBuf> {
    let path_str = path.to_string_lossy();
    let idx = path_str.rfind("/src/")?;
    Some(PathBuf::from(&path_str[idx + 1..]))
}

/// Optional-path variant of [`same_file`].
pub fn same_file_opt(left: Option<&PathBuf>, right: Option<&PathBuf>) -> bool {
    match (left, right) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(a), Some(b)) => same_file(a, b),
    }
}
