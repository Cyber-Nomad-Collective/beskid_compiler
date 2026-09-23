//! Source-unit identity, AST node keys, and their trace formatting.

use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::super::queries::{node_kind, node_span};
use super::*;

/// Source-unit identity, interned by a normalized absolute logical path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug, persist)]
pub struct SourceUnitId {
    #[get(interned_path)]
    #[returns(ref)]
    path: PathBuf,
}

impl SourceUnitId {
    /// Normalize the deepest existing ancestor before interning the remaining logical suffix.
    ///
    /// This makes new LSP files stable when they are first named through a symlink and later
    /// created on disk.
    pub fn new(db: &dyn Db, path: PathBuf) -> Self {
        Self::intern_path(db, normalized_source_path(&path))
    }

    pub fn path(self, db: &dyn Db) -> &PathBuf {
        self.interned_path(db)
    }
}

/// Format a generation-safe syntax key as `path#gN:nN` for traces and diagnostics.
pub fn format_ast_node_key(db: &dyn Db, key: AstNodeKey) -> String {
    key.display_label(key.unit.path(db).display())
}

/// Format a lowering/diagnostic site as `path#gN:nN Construct@line:col-line:col`.
///
/// Falls back to `Unknown` / `?:?-?:?` when kind or span facts are unavailable so the key is
/// still actionable without requiring a second query at the call site.
pub fn format_ast_node_site(db: &dyn Db, key: AstNodeKey) -> String {
    let label = format_ast_node_key(db, key);
    let construct =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "Unknown".to_owned());
    let range = node_span(db, key).ok().flatten().map(format_source_span_range).unwrap_or_else(|| "?:?-?:?".to_owned());
    format!("{label} {construct}@{range}")
}

/// Format a source span as `line:col-line:col` (1-based endpoints).
pub fn format_source_span_range(span: SourceSpan) -> String {
    format!("{}:{}-{}:{}", span.line_col_start.0, span.line_col_start.1, span.line_col_end.0, span.line_col_end.1)
}

/// Format a source-level trace entry as `<source_label>:<line>:<col> (<Construct>)`.
///
/// `source_label` is a short name for the containing source unit (typically the logical path
/// from the assembly). This intentionally omits the generation-safe `#gN:nN` suffix and byte
/// range so that `at`-style traces remain readable under `BESKID_COMPILER_TRACE`.
pub fn format_ast_node_trace(db: &dyn Db, key: AstNodeKey, source_label: &str) -> String {
    let construct =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "Unknown".to_owned());
    let position = node_span(db, key)
        .ok()
        .flatten()
        .map(|span| format!("{}:{}", span.line_col_start.0, span.line_col_start.1))
        .unwrap_or_else(|| "?:?".to_owned());
    format!("{source_label}:{position} ({construct})")
}

#[cfg(test)]
mod ast_node_site_format_tests {
    use super::{SourceSpan, format_source_span_range};

    #[test]
    fn formats_line_column_range() {
        let span = SourceSpan { start: 100, end: 140, line_col_start: (52, 5), line_col_end: (55, 6) };
        assert_eq!(format_source_span_range(span), "52:5-55:6");
    }
}

fn normalized_source_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    };
    let mut ancestor = absolute.clone();
    let mut suffix = Vec::<OsString>::new();

    loop {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(leaf) = ancestor.file_name().map(ToOwned::to_owned) else {
            return absolute;
        };
        suffix.push(leaf);
        if !ancestor.pop() {
            return absolute;
        }
    }
}

/// Generation-safe key for a syntax node in an interned source unit.
pub type AstNodeKey = beskid_analysis::syntax::AstNodeKey<SourceUnitId>;
