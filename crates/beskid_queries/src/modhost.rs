//! Persisted generation identities; execution belongs to the registered prepare/scope authority.

/// Spec: `syntax_generation_id` — bumped when entry file text changes.
#[salsa::input(persist)]
pub struct ModHostSyntaxGenerationId {
    pub path: String,
    pub generation: u64,
}

/// Spec: `manifest_generation_id` — hash of manifest/lockfile bytes.
#[salsa::input(persist)]
pub struct ManifestGenerationId {
    #[returns(ref)]
    pub digest: String,
}
