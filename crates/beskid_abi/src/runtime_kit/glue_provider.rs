//! Installed Glue provider identity is separate from ordinary ABI-v5 kit validity.
use super::{BuildProfile, ResolvedRuntimeKit, RuntimeArtifact};
use crate::abi_v5::TargetMetadata;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const GLUE_PROVIDER_MANIFEST_V1: &str = "glue-provider-v1.json";
pub const GLUE_OWNER_ISSUER_V1_SYMBOL: &str = "beskid_glue_v1_next_identity";

pub fn canonical_glue_issuer_source_sha256() -> String {
    let mut digest = Sha256::new();
    digest.update(b"beskid.Glue.OwnerIssuer.V1\0");
    for (name, bytes) in [
        ("owner_identity_v1.c", include_bytes!("../../../../runtime/Glue/owner_identity_v1.c").as_slice()),
        ("owner_identity_v1.h", include_bytes!("../../../../runtime/Glue/owner_identity_v1.h").as_slice()),
    ] {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    format!("{:x}", digest.finalize())
}

/// Build-time tool evidence; installed consumers do not resolve build-host paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueProviderToolV1 {
    pub role: String,
    pub executable_sha256: String,
    pub version_output_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueProviderManifestV1 {
    pub schema_version: u32,
    pub issuer_version: u32,
    pub target: String,
    pub profile: BuildProfile,
    pub runtime_layout_sha256: String,
    pub runtime_source_sha256: String,
    pub issuer_source_sha256: String,
    pub shared_library: RuntimeArtifact,
    pub shared_import_library: Option<RuntimeArtifact>,
    pub build_tools: Vec<GlueProviderToolV1>,
}
#[derive(Debug)]
pub enum GlueProviderError {
    Invalid(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}
impl std::fmt::Display for GlueProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid canonical Glue provider V1: {message}"),
            Self::Io(error) => write!(f, "Glue provider manifest I/O failed: {error}"),
            Self::Json(error) => write!(f, "Glue provider manifest decoding failed: {error}"),
        }
    }
}
impl std::error::Error for GlueProviderError {}
impl From<std::io::Error> for GlueProviderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for GlueProviderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl GlueProviderManifestV1 {
    pub fn validate_against_kit(&self, kit: &ResolvedRuntimeKit) -> Result<(), GlueProviderError> {
        self.validate_against_metadata(&kit.metadata)
    }
    pub(crate) fn from_build(
        metadata: &super::RuntimeKitMetadata,
        build_tools: Vec<GlueProviderToolV1>,
    ) -> Result<Self, GlueProviderError> {
        if metadata.source_hash != crate::runtime_source::canonical_runtime_source_hash() {
            return Err(GlueProviderError::Invalid(
                "provider must use compiler-owned canonical runtime sources".into(),
            ));
        }
        let manifest = Self {
            schema_version: 1,
            issuer_version: 1,
            target: metadata.target.triple.as_str().into(),
            profile: metadata.profile,
            runtime_layout_sha256: metadata.layout_hash.clone(),
            runtime_source_sha256: metadata.source_hash.clone(),
            issuer_source_sha256: canonical_glue_issuer_source_sha256(),
            shared_library: metadata.artifacts.shared_library.clone(),
            shared_import_library: metadata.artifacts.shared_import_library.clone(),
            build_tools,
        };
        manifest.validate_against_metadata(metadata)?;
        Ok(manifest)
    }
    fn validate_against_metadata(&self, metadata: &super::RuntimeKitMetadata) -> Result<(), GlueProviderError> {
        if self.schema_version != 1
            || self.issuer_version != 1
            || self.target != metadata.target.triple.as_str()
            || self.profile != metadata.profile
            || self.runtime_layout_sha256 != metadata.layout_hash
            || self.runtime_source_sha256 != metadata.source_hash
            || self.issuer_source_sha256 != canonical_glue_issuer_source_sha256()
            || self.shared_library != metadata.artifacts.shared_library
            || self.shared_import_library != metadata.artifacts.shared_import_library
        {
            return Err(GlueProviderError::Invalid("source/target/ABI/shared payload identity differs".into()));
        }
        if !metadata.loader_required_exports.iter().any(|s| s == GLUE_OWNER_ISSUER_V1_SYMBOL) {
            return Err(GlueProviderError::Invalid("canonical loader lacks owner issuer V1".into()));
        }
        let roles = self.build_tools.iter().map(|tool| tool.role.as_str()).collect::<Vec<_>>();
        if roles != ["native_provider_compiler", "shared_provider_linker"] {
            return Err(GlueProviderError::Invalid("exact ordered compiler/linker tool evidence is required".into()));
        }
        for tool in &self.build_tools {
            for digest in [&tool.executable_sha256, &tool.version_output_sha256] {
                if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
                    return Err(GlueProviderError::Invalid("invalid tool digest".into()));
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct ResolvedGlueSharedProvider {
    pub kit: ResolvedRuntimeKit,
    pub manifest: GlueProviderManifestV1,
    pub link_library: PathBuf,
    pub shared_library: PathBuf,
}

/// Resolve an exact installed shared provider. No ambient, static or missing-sidecar fallback.
/// Receipt self-consistency is not release qualification: qualification independently pins
/// the source/tool/provider closure and audits actual binary dynamic dependencies.
pub fn resolve_glue_shared_provider(
    prefix: &Path,
    target: &TargetMetadata,
    profile: BuildProfile,
) -> Result<ResolvedGlueSharedProvider, GlueProviderError> {
    let kit = crate::runtime_source::resolve_canonical_runtime_kit(prefix, target, profile)
        .map_err(|error| GlueProviderError::Invalid(format!("canonical runtime kit failed: {error:?}")))?;
    let path = kit.root.join(GLUE_PROVIDER_MANIFEST_V1);
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(GlueProviderError::Invalid("manifest is not a bounded regular file".into()));
    }
    let manifest: GlueProviderManifestV1 = serde_json::from_slice(&std::fs::read(path)?)?;
    manifest.validate_against_kit(&kit)?;
    let link_library = kit.shared_import_library.clone().unwrap_or_else(|| kit.shared_library.clone());
    let shared_library = kit.shared_library.clone();
    Ok(ResolvedGlueSharedProvider { kit, manifest, link_library, shared_library })
}
