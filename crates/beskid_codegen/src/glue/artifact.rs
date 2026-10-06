//! Deterministic generated files, distinct from machine-local build execution evidence.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}
impl GeneratedFile {
    pub fn new(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        Self { path: path.into(), sha256: digest(&bytes), bytes }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueSourceAuthority {
    pub logical_unit: String,
    pub typed_unit_sha256: String,
    pub span_start: usize,
    pub span_end: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueOpaqueType {
    pub brand_sha256:String,
    pub library:String,
    pub nullable:bool,
    pub constructor:String,
    pub reader:String,
    pub descriptor:String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GluePhysicalType {
    pub logical: String,
    pub slots: Vec<String>,
    pub checked: bool,
    pub opaque:Option<GlueOpaqueType>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueBindingManifest {
    pub identity_sha256: String,
    pub direction: String,
    pub library: String,
    pub symbol: String,
    pub body_symbol: String,
    pub shape_id: u64,
    pub parameters: Vec<GluePhysicalType>,
    pub result: GluePhysicalType,
    pub source: GlueSourceAuthority,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueManifest {
    pub schema_version: u32,
    pub backend: String,
    pub target: String,
    pub runtime_abi: u32,
    pub runtime_linkage: String,
    pub owner_issuer_version: u32,
    pub native_width_bits: u32,
    pub compiled_source_sha256: String,
    pub bindings: Vec<GlueBindingManifest>,
    pub required_tools: Vec<String>,
    pub required_native_adapters: Vec<String>,
    pub expected_outputs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueArtifact {
    pub manifest: GlueManifest,
    pub files: Vec<GeneratedFile>,
    pub sha256: String,
}
#[derive(Debug, thiserror::Error)]
pub enum GlueArtifactError {
    #[error("invalid generated Glue artifact: {0}")]
    Invalid(String),
    #[error("Glue artifact JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Exact owning-library transport identity, avoiding global basename collisions.
pub fn owned_release_symbol(library: &str) -> String {
    format!("beskid_glue_release_{}", digest(library.as_bytes()))
}
pub fn checked_invocation_symbol(binding: &GlueBindingManifest) -> String {
    format!("beskid_glue_checked_{}", binding.identity_sha256)
}
pub fn checked_invocation_symbols(manifest: &GlueManifest) -> Vec<String> {
    manifest.bindings.iter().filter(|binding| binding.direction == "export").map(checked_invocation_symbol).collect()
}
pub fn requires_checked_primary(binding: &GlueBindingManifest) -> bool {
    binding.result.opaque.is_some() || binding.parameters.iter().any(|ty|ty.opaque.is_some()) || binding.parameters.iter().any(|ty| matches!(ty.logical.as_str(), "utf8" | "bytes"))
        && !matches!(binding.result.logical.as_str(), "utf8" | "bytes")
}
pub fn export_transport_symbol(binding: &GlueBindingManifest) -> String {
    if requires_checked_primary(binding) { checked_invocation_symbol(binding) } else { binding.symbol.clone() }
}
pub fn owned_release_symbols(manifest: &GlueManifest) -> Vec<String> {
    let mut symbols=std::collections::BTreeSet::new();
    for binding in &manifest.bindings {
        if binding.direction=="export"&&matches!(binding.result.logical.as_str(),"utf8"|"bytes") {
            symbols.insert(owned_release_symbol(&binding.library));
        }
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            if let Some(opaque)=&physical.opaque {
                symbols.insert(format!("beskid_glue_opaque_{}_release",opaque.brand_sha256));
            }
        }
    }
    symbols.into_iter().collect()
}
pub fn binding_shape_digest(source: &str, binding: &GlueBindingManifest) -> Result<String, GlueArtifactError> {
    let mut logical = binding.clone();
    logical.identity_sha256.clear();
    logical.body_symbol.clear();
    logical.shape_id = 0;
    Ok(digest(&serde_json::to_vec(&(source, logical))?))
}
pub fn shape_id(digest: &str) -> Result<u64, GlueArtifactError> {
    u64::from_str_radix(digest.get(..16).unwrap_or(""), 16)
        .ok()
        .filter(|id| *id != 0)
        .ok_or_else(|| GlueArtifactError::Invalid("zero or invalid shape identity".into()))
}
impl GlueArtifact {
    pub fn new(manifest: GlueManifest, mut files: Vec<GeneratedFile>) -> Result<Self, GlueArtifactError> {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let sha256 = digest(&serde_json::to_vec(&(&manifest, &files))?);
        let value = Self { manifest, files, sha256 };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), GlueArtifactError> {
        let invalid = |message: &str| GlueArtifactError::Invalid(message.into());
        if self.manifest.schema_version != 1
            || self.manifest.backend != "rust"
            || self.manifest.runtime_abi != 5
            || self.manifest.runtime_linkage != "canonical_shared_provider"
            || self.manifest.owner_issuer_version != 1
            || self.manifest.native_width_bits != 64
            || self.manifest.compiled_source_sha256.len() != 64
            || !self.manifest.compiled_source_sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid("schema/target/runtime provider profile differs"));
        }
        if self.files.is_empty()
            || self.files.len() > 1024
            || self.manifest.bindings.is_empty()
            || self.manifest.bindings.len() > 4096
        {
            return Err(invalid("file/binding bound differs"));
        }
        if !matches!(
            self.manifest.target.as_str(),
            "aarch64-apple-darwin" | "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
        ) {
            return Err(invalid("unsupported target profile"));
        }
        let ordered = |values: &[String]| values.windows(2).all(|w| w[0] < w[1]);
        if !ordered(&self.manifest.required_tools)
            || !ordered(&self.manifest.required_native_adapters)
            || !ordered(&self.manifest.expected_outputs)
            || self.manifest.required_tools
                != if self.manifest.target == "x86_64-pc-windows-msvc" {
                    vec!["cl".to_owned(), "rustc".to_owned()]
                } else {
                    vec!["cc".to_owned(), "rustc".to_owned()]
                }
        {
            return Err(invalid("tool/output closure differs"));
        }
        let mut previous_binding = None;
        let mut shapes = BTreeSet::new();
        for binding in &self.manifest.bindings {
            let ordering = (&binding.library, &binding.symbol, &binding.identity_sha256);
            if previous_binding.is_some_and(|p| p >= ordering) {
                return Err(invalid("binding order differs"));
            }
            previous_binding = Some(ordering);
            if !matches!(binding.direction.as_str(), "import" | "export")
                || binding.source.logical_unit.is_empty()
                || binding.source.span_start > binding.source.span_end
                || binding.source.typed_unit_sha256.len() != 64
                || !binding.source.typed_unit_sha256.bytes().all(|c| c.is_ascii_hexdigit())
                || binding.shape_id == 0
                || !binding.body_symbol.starts_with("__beskid_glue_body_")
            {
                return Err(invalid("binding source authority differs"));
            }
            for ty in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
                match (&ty.opaque,ty.logical.starts_with("opaque:")) {
                    (Some(opaque),true)=>{
                        if opaque.brand_sha256.len()!=64 || !opaque.brand_sha256.bytes().all(|b|b.is_ascii_hexdigit()) ||
                            opaque.library.is_empty() || opaque.library.contains('\0') ||
                            opaque.constructor!=format!("beskid_glue_handle_{}_construct",opaque.brand_sha256) ||
                            opaque.reader!=format!("beskid_glue_handle_{}_read",opaque.brand_sha256) ||
                            opaque.descriptor!=format!("beskid_glue_handle_{}_descriptor",opaque.brand_sha256) ||
                            ty.slots!=["uint64_t"] {
                            return Err(invalid("opaque source transport metadata differs"));
                        }
                    },
                    (None,false)=>{},
                    _=>return Err(invalid("opaque source transport metadata absent or attached to ordinary type")),
                }
            }
            let mut identity = binding.clone();
            identity.identity_sha256.clear();
            if binding.identity_sha256 != digest(&serde_json::to_vec(&identity)?) {
                return Err(invalid("binding identity differs"));
            }
            let shape = binding_shape_digest(&self.manifest.compiled_source_sha256, binding)?;
            if binding.shape_id != shape_id(&shape)?
                || binding.body_symbol != format!("__beskid_glue_body_{shape}")
                || !shapes.insert(binding.shape_id)
            {
                return Err(invalid("compiled shape/body identity differs or collides"));
            }
        }
        let manifest_file = self
            .files
            .iter()
            .find(|f| f.path == "glue-manifest.json")
            .ok_or_else(|| invalid("missing manifest file"))?;
        if serde_json::from_slice::<GlueManifest>(&manifest_file.bytes)? != self.manifest {
            return Err(invalid("manifest file differs"));
        }
        let mut paths = BTreeSet::new();
        let mut total = 0usize;
        let mut previous = None;
        for file in &self.files {
            if file.path.is_empty()
                || file.path.len() > 4096
                || file.path.starts_with('/')
                || file.path.contains('\\')
                || file.path.contains(':')
                || file.path.split('/').any(|p| p.is_empty() || p == "." || p == "..")
                || !paths.insert(file.path.as_str())
            {
                return Err(invalid("unsafe or duplicate generated path"));
            }
            if previous.is_some_and(|p: &str| p >= file.path.as_str()) {
                return Err(invalid("generated paths are not ordered"));
            }
            previous = Some(&file.path);
            total = total.checked_add(file.bytes.len()).ok_or_else(|| invalid("generated bytes overflow"))?;
            if total > 16 * 1024 * 1024 || file.sha256 != digest(&file.bytes) {
                return Err(invalid("generated bytes exceed bound or digest differs"));
            }
        }
        if self.sha256 != digest(&serde_json::to_vec(&(&self.manifest, &self.files))?) {
            return Err(invalid("artifact digest differs"));
        }
        Ok(())
    }
}
