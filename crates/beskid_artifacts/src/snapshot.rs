//! Serializable unit artifact payloads (postcard wire format).

use serde::{Deserialize, Serialize};

use crate::manifest::{ARTIFACT_SCHEMA_VERSION, UnitArtifactMeta};

/// Post-macro-expand AST unit snapshot (`ast.bin`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AstUnitSnapshot {
    pub schema_version: u32,
    pub meta: UnitArtifactMeta,
    /// Postcard-encoded `Spanned<Program>` from `beskid_analysis::artifacts`.
    pub program_wire: Vec<u8>,
}

/// Lowered HIR unit snapshot (`hir.bin`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HirUnitSnapshot {
    pub schema_version: u32,
    pub content_fingerprint: String,
    /// Postcard-encoded `Spanned<HirProgram>` from `beskid_analysis::artifacts`.
    pub hir_wire: Vec<u8>,
}

/// Combined record for in-memory / API transport.
#[derive(Debug, Clone)]
pub struct UnitArtifactRecord {
    pub meta: UnitArtifactMeta,
    pub ast: AstUnitSnapshot,
    pub hir: HirUnitSnapshot,
}

impl AstUnitSnapshot {
    pub fn new(meta: UnitArtifactMeta, program_wire: Vec<u8>) -> Self {
        Self {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            meta,
            program_wire,
        }
    }
}

impl HirUnitSnapshot {
    pub fn new(content_fingerprint: String, hir_wire: Vec<u8>) -> Self {
        Self {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            content_fingerprint,
            hir_wire,
        }
    }
}

pub fn encode_ast(snapshot: &AstUnitSnapshot) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(snapshot)
}

pub fn decode_ast(bytes: &[u8]) -> Result<AstUnitSnapshot, postcard::Error> {
    postcard::from_bytes(bytes)
}

pub fn encode_hir(snapshot: &HirUnitSnapshot) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(snapshot)
}

pub fn decode_hir(bytes: &[u8]) -> Result<HirUnitSnapshot, postcard::Error> {
    postcard::from_bytes(bytes)
}
