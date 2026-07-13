//! Unit artifact cache: content-addressed fingerprints and on-disk layout under
//! `{project}/obj/beskid/cache/salsa/units/{content_fp}/`.

pub mod fingerprint;
pub mod manifest;
pub mod persistence;
pub mod snapshot;

pub use fingerprint::{content_fingerprint, grammar_revision};
pub use manifest::{ARTIFACT_SCHEMA_VERSION, ArtifactManifest, UnitArtifactMeta};
pub use persistence::{ArtifactStore, UnitArtifactPaths};
pub use snapshot::{AstUnitSnapshot, UnitArtifactRecord, decode_ast, encode_ast};
