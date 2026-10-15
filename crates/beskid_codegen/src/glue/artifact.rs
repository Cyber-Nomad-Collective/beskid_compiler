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
/// One field of a by-value record: its scalar logical name, C type and C ABI profile offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueRecordField {
    pub name: String,
    pub logical: String,
    pub c_type: String,
    pub offset: u64,
}
/// A `[GlueRecord]` crossing by value as the C struct `c_name`. Size, alignment and offsets are
/// the C ABI profile binding of the lowered record shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueRecordType {
    pub record_sha256: String,
    pub identity: String,
    pub c_name: String,
    pub fields: Vec<GlueRecordField>,
    pub size: u64,
    pub alignment: u64,
    pub constructor: String,
    pub reader: String,
    /// `[RustOwner(Path:..)]` Rust type of the record, when declared.
    pub rust_owner: Option<String>,
}
/// A fieldless `[GlueEnum]` crossing as its `uint32_t` source-order variant index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueEnumType {
    pub enum_sha256: String,
    pub identity: String,
    pub variants: Vec<String>,
    pub constructor: String,
    pub reader: String,
    /// `[RustOwner(Path:..)]` Rust type of the enum, when declared.
    pub rust_owner: Option<String>,
}
/// A call-scoped callback: `int32_t (*c_name)(uint64_t context, parameters..., result *out)`
/// plus the context token. Parameters and result are scalar or enum physical types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlueCallbackType {
    pub callback_sha256: String,
    pub c_name: String,
    pub invoke: String,
    pub parameters: Vec<GluePhysicalType>,
    pub result: GluePhysicalType,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GluePhysicalType {
    pub logical: String,
    pub slots: Vec<String>,
    pub checked: bool,
    pub opaque:Option<GlueOpaqueType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<GlueRecordType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enumeration: Option<GlueEnumType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<Box<GlueCallbackType>>,
}
impl GluePhysicalType {
    /// A scalar, view or unit physical type with no nominal metadata.
    pub fn plain(logical: impl Into<String>, slots: Vec<String>, checked: bool) -> Self {
        Self { logical: logical.into(), slots, checked, opaque: None, record: None, enumeration: None, callback: None }
    }
}
/// Symbol of the generated Cranelift record constructor `void *(const C *)`.
pub fn record_constructor_symbol(record_sha256: &str) -> String {
    format!("beskid_glue_record_{record_sha256}_construct")
}
/// Symbol of the generated Cranelift record reader `int32_t (void *, C *)`.
pub fn record_reader_symbol(record_sha256: &str) -> String {
    format!("beskid_glue_record_{record_sha256}_read")
}
/// Symbol of the generated Cranelift enum constructor `void *(uint32_t)`.
pub fn enum_constructor_symbol(enum_sha256: &str) -> String {
    format!("beskid_glue_enum_{enum_sha256}_construct")
}
/// Symbol of the generated Cranelift enum reader `int32_t (void *, uint32_t *)`.
pub fn enum_reader_symbol(enum_sha256: &str) -> String {
    format!("beskid_glue_enum_{enum_sha256}_read")
}
/// Every record, enum and callback physical type of `binding`, callback members included.
pub fn nominal_values(binding: &GlueBindingManifest) -> Vec<&GluePhysicalType> {
    let mut values = Vec::new();
    for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
        if let Some(callback) = &physical.callback {
            values.extend(callback.parameters.iter().chain(std::iter::once(&callback.result)));
        }
        values.push(physical);
    }
    values
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
    /// Import: the Rust owner callable is declared `[RustOwner(Fallible:true)]` and its nonzero
    /// status may be recovered by a checked Glue invocation. Export: the body reaches such an
    /// import, so its checked invocation runs the body in a Glue checked scope and returns the
    /// recorded foreign status instead of trapping.
    pub fallible: bool,
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
/// Checked counterpart of a fallible export body. It runs only inside the Glue checked scope that
/// the export's checked invocation enters; it is linked into the image and never exported.
pub fn checked_body_symbol(binding: &GlueBindingManifest) -> String {
    format!("{}_checked_body", binding.body_symbol)
}
pub fn checked_invocation_symbols(manifest: &GlueManifest) -> Vec<String> {
    manifest.bindings.iter().filter(|binding| binding.direction == "export").map(checked_invocation_symbol).collect()
}
/// Whether an export's primary C entry is its checked invocation: opaque handles, records and
/// enums need managed construction or domain checks that only a status can report, and managed
/// inputs with a scalar result need a status to report a failed input.
pub fn requires_checked_primary(binding: &GlueBindingManifest) -> bool {
    let value = |ty: &GluePhysicalType| ty.opaque.is_some() || ty.record.is_some() || ty.enumeration.is_some();
    value(&binding.result)
        || binding.parameters.iter().any(value)
        || binding.parameters.iter().any(|ty| matches!(ty.logical.as_str(), "utf8" | "bytes"))
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
/// The folded shape of an opaque brand: the first eight bytes of the brand digest as a
/// little-endian word, exactly as the owner runtime (`opaque_shape` in `owner_identity_v1.c`) and
/// the Rust owner admission fold it. A consumer image, its loader-issued shape closure and the
/// owner side must agree on this one fold or the opaque domain binding is rejected.
pub fn brand_shape(brand_sha256: &str) -> Result<u64, GlueArtifactError> {
    let mut bytes = [0u8; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = brand_sha256
            .get(index * 2..index * 2 + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .ok_or_else(|| GlueArtifactError::Invalid("invalid opaque brand digest".into()))?;
    }
    if brand_sha256.len() != 64 || !brand_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GlueArtifactError::Invalid("invalid opaque brand digest".into()));
    }
    let shape = u64::from_le_bytes(bytes);
    (shape != 0).then_some(shape).ok_or_else(|| GlueArtifactError::Invalid("zero opaque brand shape".into()))
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
            for ty in nominal_values(binding) {
                validate_nominal(ty).map_err(|message| invalid(message))?;
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

fn hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// Record, enum and callback metadata is consistent with its logical name and slots; a nominal
/// marker on an ordinary type, or a missing one, is rejected.
fn validate_nominal(ty: &GluePhysicalType) -> Result<(), &'static str> {
    let kinds = usize::from(ty.opaque.is_some())
        + usize::from(ty.record.is_some())
        + usize::from(ty.enumeration.is_some())
        + usize::from(ty.callback.is_some());
    if kinds > 1 {
        return Err("physical type carries two nominal transports");
    }
    match (&ty.record, ty.logical.strip_prefix("record:")) {
        (Some(record), Some(identity)) => {
            if !hex64(&record.record_sha256)
                || record.identity != identity
                || record.c_name != format!("GlueRecord_{}", record.record_sha256)
                || ty.slots != [record.c_name.clone()]
                || record.constructor != record_constructor_symbol(&record.record_sha256)
                || record.reader != record_reader_symbol(&record.record_sha256)
                || record.fields.is_empty()
                || record.fields.len() > 64
            {
                return Err("record transport metadata differs");
            }
        }
        (None, None) => {}
        _ => return Err("record transport metadata absent or attached to ordinary type"),
    }
    match (&ty.enumeration, ty.logical.strip_prefix("enum:")) {
        (Some(enumeration), Some(identity)) => {
            if !hex64(&enumeration.enum_sha256)
                || enumeration.identity != identity
                || ty.slots != ["uint32_t"]
                || enumeration.constructor != enum_constructor_symbol(&enumeration.enum_sha256)
                || enumeration.reader != enum_reader_symbol(&enumeration.enum_sha256)
                || enumeration.variants.is_empty()
                || enumeration.variants.len() > 4096
            {
                return Err("enum transport metadata differs");
            }
        }
        (None, None) => {}
        _ => return Err("enum transport metadata absent or attached to ordinary type"),
    }
    match (&ty.callback, ty.logical.strip_prefix("callback:")) {
        (Some(callback), Some(digest)) => {
            if !hex64(&callback.callback_sha256)
                || callback.callback_sha256 != digest
                || callback.c_name != format!("GlueCallback_{digest}")
                || callback.invoke != format!("beskid_glue_callback_{digest}_invoke")
                || ty.slots != [callback.c_name.clone(), "uint64_t".to_owned()]
                || callback.parameters.len() > 16
                || callback.parameters.iter().chain(std::iter::once(&callback.result)).any(|member| {
                    member.callback.is_some() || member.record.is_some() || member.opaque.is_some()
                })
            {
                return Err("callback transport metadata differs");
            }
        }
        (None, None) => {}
        _ => return Err("callback transport metadata absent or attached to ordinary type"),
    }
    Ok(())
}
