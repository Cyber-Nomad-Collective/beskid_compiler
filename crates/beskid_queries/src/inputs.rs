//! Salsa inputs: file text, lockfile, compile plan snapshot.

use std::path::PathBuf;

/// Per-file source text; LSP/CLI set this on edit.
#[salsa::input]
pub struct FileText {
    pub path: PathBuf,
    #[returns(ref)]
    pub text: String,
}

/// Project-scoped session identity (roots + lockfile + target).
#[salsa::input]
pub struct ProjectSession {
    pub project_root: PathBuf,
    pub entry_path: PathBuf,
    pub target_name: String,
    #[returns(ref)]
    pub lockfile_digest: String,
}

/// Grammar/compiler revision baked into unit invalidation.
#[salsa::input]
pub struct GrammarRevision {
    #[returns(ref)]
    pub rev: String,
}
