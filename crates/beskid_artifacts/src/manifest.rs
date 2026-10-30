//! Cache manifest and per-unit metadata.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Bump when the syntax snapshot wire layout or the meaning of a stored snapshot changes.
/// Version 3: a snapshot is only ever the tree of a source that parses (earlier stores could hold
/// a parser repair of a source with a parse error).
pub const ARTIFACT_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactManifest {
    pub grammar_rev: String,
    pub compiler_version: String,
    pub schema_version: u32,
    pub persisted_units: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnitArtifactMeta {
    pub content_fingerprint: String,
    pub schema_version: u32,
    pub grammar_rev: String,
    pub logical_name: String,
    pub source_path: PathBuf,
    pub source_len: usize,
    pub imports: Vec<String>,
}
