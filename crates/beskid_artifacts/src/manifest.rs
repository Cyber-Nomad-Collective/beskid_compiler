//! Cache manifest and per-unit metadata.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Bump when the syntax snapshot wire layout changes.
pub const ARTIFACT_SCHEMA_VERSION: u32 = 2;

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
