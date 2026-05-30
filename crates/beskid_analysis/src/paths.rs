//! Canonical path comparison for scoped resolve/type/codegen tables.

use std::path::{Path, PathBuf};

/// Whether two paths refer to the same on-disk file (equality or matching canonical paths).
pub fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Optional-path variant of [`same_file`].
pub fn same_file_opt(left: Option<&PathBuf>, right: Option<&PathBuf>) -> bool {
    match (left, right) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(a), Some(b)) => same_file(a, b),
    }
}
