//! Unit artifact cache: content-addressed fingerprints and on-disk layout under
//! `{project}/obj/beskid/cache/salsa/units/{content_fp}/`.

pub mod fingerprint;
pub mod manifest;
pub mod persistence;
pub mod snapshot;

pub use fingerprint::{content_fingerprint, grammar_revision};
pub use manifest::{ArtifactManifest, UnitArtifactMeta, ARTIFACT_SCHEMA_VERSION};
pub use persistence::{ArtifactStore, UnitArtifactPaths};
pub use snapshot::{
    AstUnitSnapshot, HirUnitSnapshot, UnitArtifactRecord, decode_ast, decode_hir, encode_ast,
    encode_hir,
};
