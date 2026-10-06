//! Admission of published Glue images (Gate WEB-GLUE03).
//!
//! Each producer compiles an immutable admission record into its image before the link: the
//! consumer through a generated C object, a Rust owner through a generated `static`. The record is
//! read from the file bytes before any image code runs. A record is evidence, not trust: anyone
//! who can rewrite an image can rewrite its record. Trust comes from one digest that the host
//! receives through a channel it already trusts (the build's own result output), the way pip
//! hash-checking and fs-verity use an externally trusted digest. That digest pins the consumer
//! image and so its record. The consumer record pins every Rust owner image by its exact digest,
//! and names the provider digest. The provider and the compiler driver are resolved from the
//! installed toolchain prefix, never from a file beside the outputs.
use super::{
    AotResult, CheckedEntryWitness, DynamicShapeWitness, GlueProducerWitness, HandleTransportWitness,
    IssuedGlueImage, PreparedRustOwner, bounded_file, consumer_exports, hash, hash_file, invalid, provider_identity,
    require_exports, require_sole_provider, target_shared_image,
};
use super::loader::{GlueProcess, IssuedRustOwner};
use crate::runtime::{RuntimeBuildRequest, RuntimeLinkage, prepare_runtime};
use beskid_codegen::glue::GlueArtifact;
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Exported read-only data symbol that holds the consumer admission record.
pub(super) const CONSUMER_RECORD_SYMBOL: &str = "beskid_glue_artifact_v1_admission_record";
/// Exported read-only data symbol that holds a Rust owner admission record.
pub(super) const OWNER_RECORD_SYMBOL: &str = "beskid_glue_rust_owner_v1_admission_record";
const RECORD_MAGIC: &[u8; 16] = b"BESKID-GLUE-ADM1";
const RECORD_HEADER: u64 = 24;
const RECORD_LIMIT: u64 = 16 * 1024 * 1024;
const RECORD_FORMAT: u32 = 1;
/// Producer identity. A record from another compiler version is not admitted.
pub(super) const RECORD_COMPILER: &str = concat!("beskid_aot/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerPin {
    pub(super) library: String,
    pub(super) image_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckedEntryRecord {
    symbol: String,
    source_symbol: String,
    admission_symbol: String,
    source_parameter_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DynamicShapeRecord {
    signature: String,
    signature_sha256: String,
    payload_descriptor_getter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandleTransportRecord {
    brand: String,
    constructor: String,
    reader: String,
    descriptor: String,
}

/// Consumer producer evidence, compiled into the consumer image before linking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConsumerRecord {
    format: u32,
    compiler: String,
    library: String,
    generation: u64,
    packet: GlueArtifact,
    pub(super) provider_sha256: String,
    provider_identity: String,
    body_object_sha256: String,
    adapter_object_sha256: String,
    adapter_source_sha256: String,
    adapter_compiler_sha256: String,
    failure_admissions: Vec<String>,
    checked_entries: Vec<CheckedEntryRecord>,
    dynamic_shapes: Vec<DynamicShapeRecord>,
    handle_transports: Vec<HandleTransportRecord>,
    owners: Vec<OwnerPin>,
}

/// Rust owner producer evidence, compiled into the owner image before linking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerRecord {
    pub(super) format: u32,
    pub(super) compiler: String,
    pub(super) library: String,
    pub(super) generation: u64,
    pub(super) provider_sha256: String,
    pub(super) provider_identity: String,
    /// Framed digest of every owner build input except this record file.
    pub(super) source_sha256: String,
    pub(super) source_inventory: Vec<(String, String)>,
    pub(super) cargo_sha256: String,
    pub(super) rustc_sha256: String,
    pub(super) linker_sha256: String,
    pub(super) driver_sha256: String,
    /// `(brand sha256, destructor symbol)`.
    pub(super) destructors: Vec<(String, String)>,
    /// `(binding identity sha256, checked symbol, shape id)`.
    pub(super) checked_exports: Vec<(String, String, u64)>,
    pub(super) exports: Vec<String>,
}

impl OwnerRecord {
    pub(super) fn current_format() -> u32 {
        RECORD_FORMAT
    }
}

fn is_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str, limit: usize) -> AotResult<Vec<u8>> {
    if text.len() % 2 != 0 || text.len() / 2 > limit {
        return Err(invalid("admission record hex field has an invalid length"));
    }
    (0..text.len() / 2)
        .map(|index| {
            let pair = text.get(index * 2..index * 2 + 2).ok_or_else(|| invalid("admission record hex encoding"))?;
            if !pair.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
                return Err(invalid("admission record hex encoding"));
            }
            u8::from_str_radix(pair, 16).map_err(|_| invalid("admission record hex encoding"))
        })
        .collect()
}

fn digest_array(text: &str) -> AotResult<[u8; 32]> {
    if !is_digest(text) {
        return Err(invalid("admission record digest is not a lowercase SHA-256"));
    }
    unhex(text, 32)?.try_into().map_err(|_| invalid("admission record digest width"))
}

/// Libraries a consumer packet references through an Extern import or an opaque handle type.
fn referenced_libraries(packet: &GlueArtifact) -> BTreeSet<String> {
    let mut libraries = BTreeSet::new();
    for binding in &packet.manifest.bindings {
        if binding.direction == "import" {
            libraries.insert(binding.library.clone());
        }
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            if let Some(opaque) = &physical.opaque {
                libraries.insert(opaque.library.clone());
            }
        }
    }
    libraries
}

impl ConsumerRecord {
    /// Issued by `build_glue_artifact` before its link; `producer.native_sha256` is not part of it.
    pub(super) fn new(
        library: &str,
        packet: &GlueArtifact,
        provider_sha256: &str,
        provider_identity: &str,
        producer: &GlueProducerWitness,
        owners: &[&PreparedRustOwner],
    ) -> AotResult<Self> {
        let referenced = referenced_libraries(packet);
        let mut pins = Vec::with_capacity(owners.len());
        let mut libraries = BTreeSet::new();
        for owner in owners {
            owner.verify()?;
            if !referenced.contains(&owner.library) || !libraries.insert(owner.library.clone()) {
                return Err(invalid(format!(
                    "Rust owner `{}` is not referenced by this consumer or is pinned twice",
                    owner.library
                )));
            }
            if owner.generation != producer.syntax_generation.0 {
                return Err(invalid("Rust owner and consumer come from different source generations"));
            }
            pins.push(OwnerPin { library: owner.library.clone(), image_sha256: owner.payload_sha256.clone() });
        }
        pins.sort_by(|left, right| left.library.cmp(&right.library));
        Ok(Self {
            format: RECORD_FORMAT,
            compiler: RECORD_COMPILER.to_owned(),
            library: library.to_owned(),
            generation: producer.syntax_generation.0,
            packet: packet.clone(),
            provider_sha256: provider_sha256.to_owned(),
            provider_identity: provider_identity.to_owned(),
            body_object_sha256: producer.body_object_sha256.clone(),
            adapter_object_sha256: producer.adapter_object_sha256.clone(),
            adapter_source_sha256: producer.adapter_source_sha256.clone(),
            adapter_compiler_sha256: producer.adapter_compiler.sha256.clone(),
            failure_admissions: producer.failure_admissions.clone(),
            checked_entries: producer
                .checked_entries
                .iter()
                .map(|entry| CheckedEntryRecord {
                    symbol: entry.symbol.clone(),
                    source_symbol: entry.source_symbol.clone(),
                    admission_symbol: entry.admission_symbol.clone(),
                    source_parameter_count: entry.source_parameter_count as u64,
                })
                .collect(),
            dynamic_shapes: producer
                .dynamic_shapes
                .iter()
                .map(|shape| DynamicShapeRecord {
                    signature: hex(&shape.signature),
                    signature_sha256: hex(&shape.signature_sha256),
                    payload_descriptor_getter: shape.payload_descriptor_getter.clone(),
                })
                .collect(),
            handle_transports: producer
                .handle_transports
                .iter()
                .map(|transport| HandleTransportRecord {
                    brand: transport.brand.clone(),
                    constructor: transport.constructor.clone(),
                    reader: transport.reader.clone(),
                    descriptor: transport.descriptor.clone(),
                })
                .collect(),
            owners: pins,
        })
    }

    pub(super) fn framed(&self) -> AotResult<Vec<u8>> {
        frame(self)
    }

    /// Static checks of a record read from a pinned consumer image, then the loader witness.
    /// The adapter compiler identity stays evidence: the record names it, and the pinned
    /// digest binds it, but the loading host need not have that compiler installed.
    fn into_witness(
        self,
        target: &str,
        provider_sha256: &str,
        provider_identity: &str,
    ) -> AotResult<(GlueArtifact, Vec<OwnerPin>, GlueProducerWitness)> {
        if self.format != RECORD_FORMAT || self.compiler != RECORD_COMPILER {
            return Err(invalid(format!(
                "published Glue consumer was produced by `{}` (record format {}), not `{RECORD_COMPILER}`",
                self.compiler, self.format
            )));
        }
        self.packet.validate().map_err(|error| invalid(error.to_string()))?;
        if self.packet.manifest.target != target
            || self.packet.manifest.runtime_abi != beskid_abi::BESKID_RUNTIME_ABI_VERSION
        {
            return Err(invalid("published Glue consumer target or runtime ABI differs from the installed kit"));
        }
        if self.provider_sha256 != provider_sha256 || self.provider_identity != provider_identity {
            return Err(invalid("published Glue consumer was built against a different runtime provider"));
        }
        if self.generation == 0 {
            return Err(invalid("published Glue consumer record has no source generation"));
        }
        let adapter =
            self.packet.files.iter().find(|file| file.path == "native/adapters.c").ok_or_else(|| {
                invalid("published Glue consumer packet lacks its generated adapter source")
            })?;
        for digest in [
            &self.body_object_sha256,
            &self.adapter_object_sha256,
            &self.adapter_source_sha256,
            &self.adapter_compiler_sha256,
        ] {
            if !is_digest(digest) {
                return Err(invalid("published Glue consumer record digest is malformed"));
            }
        }
        if adapter.sha256 != self.adapter_source_sha256 {
            return Err(invalid("published Glue consumer adapter source differs from its packet"));
        }
        let mut libraries = BTreeSet::new();
        for pin in &self.owners {
            if !is_digest(&pin.image_sha256) || !libraries.insert(pin.library.clone()) {
                return Err(invalid("published Glue consumer owner pins are malformed or duplicated"));
            }
        }
        if libraries != referenced_libraries(&self.packet) {
            return Err(invalid("published Glue consumer does not pin exactly one owner per referenced library"));
        }
        if self.failure_admissions.len() > 4096
            || self.checked_entries.len() > 4096
            || self.dynamic_shapes.len() > 4096
            || self.handle_transports.len() > 4096
        {
            return Err(invalid("published Glue consumer record exceeds closure bounds"));
        }
        let mut dynamic_shapes = Vec::with_capacity(self.dynamic_shapes.len());
        for shape in self.dynamic_shapes {
            dynamic_shapes.push(DynamicShapeWitness {
                signature: unhex(&shape.signature, 1024 * 1024)?,
                signature_sha256: digest_array(&shape.signature_sha256)?,
                payload_descriptor_getter: shape.payload_descriptor_getter,
            });
        }
        let producer = GlueProducerWitness {
            syntax_generation: beskid_analysis::syntax::SyntaxGenerationId(self.generation),
            body_object_sha256: self.body_object_sha256,
            adapter_object_sha256: self.adapter_object_sha256,
            adapter_source_sha256: self.adapter_source_sha256,
            adapter_compiler: crate::linker::LinkToolReceipt {
                executable: PathBuf::new(),
                sha256: self.adapter_compiler_sha256,
            },
            native_sha256: String::new(),
            failure_admissions: self.failure_admissions,
            checked_entries: self
                .checked_entries
                .into_iter()
                .map(|entry| -> AotResult<CheckedEntryWitness> {
                    Ok(CheckedEntryWitness {
                        symbol: entry.symbol,
                        source_symbol: entry.source_symbol,
                        admission_symbol: entry.admission_symbol,
                        source_parameter_count: usize::try_from(entry.source_parameter_count)
                            .map_err(|_| invalid("checked entry arity"))?,
                    })
                })
                .collect::<AotResult<_>>()?,
            dynamic_shapes,
            handle_transports: self
                .handle_transports
                .into_iter()
                .map(|transport| HandleTransportWitness {
                    brand: transport.brand,
                    constructor: transport.constructor,
                    reader: transport.reader,
                    descriptor: transport.descriptor,
                })
                .collect(),
        };
        Ok((self.packet, self.owners, producer))
    }
}

/// `magic(16) | payload length u64 LE | canonical JSON payload`.
fn frame<T: Serialize>(record: &T) -> AotResult<Vec<u8>> {
    let payload = serde_json::to_vec(record).map_err(|error| invalid(error.to_string()))?;
    if payload.len() as u64 > RECORD_LIMIT {
        return Err(invalid("Glue admission record exceeds its bound"));
    }
    let mut framed = Vec::with_capacity(payload.len() + RECORD_HEADER as usize);
    framed.extend_from_slice(RECORD_MAGIC);
    framed.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

pub(super) fn frame_owner_record(record: &OwnerRecord) -> AotResult<Vec<u8>> {
    frame(record)
}

/// C translation unit that defines the exported read-only record array.
pub(super) fn record_c_source(symbol: &str, framed: &[u8]) -> String {
    let mut source = format!("#include <stdint.h>\nconst uint8_t {symbol}[{}]={{", framed.len());
    for byte in framed {
        source.push_str(&byte.to_string());
        source.push(',');
    }
    source.push_str("};\n");
    source
}

/// Rust module that defines the exported read-only record array.
pub(super) fn record_rust_source(symbol: &str, framed: &[u8]) -> String {
    let mut literal = String::with_capacity(framed.len() * 4);
    for byte in framed {
        literal.push_str(&format!("\\x{byte:02x}"));
    }
    format!(
        "#[allow(non_upper_case_globals)]\n#[unsafe(no_mangle)]\npub static {symbol}: [u8; {}] = *b\"{literal}\";\n",
        framed.len()
    )
}

/// The file bytes at `address`, from exactly one section that maps them.
fn mapped<'d>(native: &object::File<'d>, address: u64, size: u64) -> AotResult<&'d [u8]> {
    let mut found = None;
    for section in native.sections() {
        if let Some(bytes) = section.data_range(address, size).map_err(|error| invalid(error.to_string()))? {
            if found.is_some() {
                return Err(invalid("Glue admission record address maps to several sections"));
            }
            found = Some(bytes);
        }
    }
    found.ok_or_else(|| invalid("Glue admission record is not backed by file bytes"))
}

/// Reads and decodes the record that `symbol` exports, without running any image code. The
/// payload must be the canonical encoding of the decoded record.
pub(super) fn read_record<T: DeserializeOwned + Serialize>(native: &object::File<'_>, symbol: &str) -> AotResult<T> {
    let name = if native.format() == object::BinaryFormat::MachO { format!("_{symbol}") } else { symbol.to_owned() };
    let exports = native.exports().map_err(|error| invalid(error.to_string()))?;
    let mut matches = exports.iter().filter(|export| export.name() == name.as_bytes());
    let (Some(export), None) = (matches.next(), matches.next()) else {
        return Err(invalid(format!("Glue image has no unique admission record `{symbol}`")));
    };
    let header = mapped(native, export.address(), RECORD_HEADER)?;
    if &header[..16] != RECORD_MAGIC {
        return Err(invalid("Glue admission record has an unknown framing"));
    }
    let length = u64::from_le_bytes(header[16..24].try_into().map_err(|_| invalid("record length width"))?);
    if length == 0 || length > RECORD_LIMIT {
        return Err(invalid("Glue admission record length is out of bounds"));
    }
    let framed = mapped(native, export.address(), RECORD_HEADER + length)?;
    let payload = &framed[RECORD_HEADER as usize..];
    let record: T = serde_json::from_slice(payload).map_err(|error| invalid(format!("Glue admission record: {error}")))?;
    if serde_json::to_vec(&record).map_err(|error| invalid(error.to_string()))? != payload {
        return Err(invalid("Glue admission record is not canonically encoded"));
    }
    Ok(record)
}

/// Request to admit a consumer image and its Rust owner images from published files.
pub struct PublishedGlueRequest<'a> {
    /// The installed kit the images were built against. Its validated canonical shared provider
    /// is the only provider, and `<prefix>/bin` holds the compiler driver the owners name.
    pub runtime: &'a RuntimeBuildRequest,
    /// Published consumer image.
    pub consumer: &'a Path,
    /// Lowercase SHA-256 of the consumer image, received through a channel the host trusts
    /// (for example the `sha256` line that `beskid build --backend glue-rust` prints). It is the
    /// only trust root: no file beside the outputs grants admission.
    pub consumer_sha256: &'a str,
    /// Published Rust owner images. Each must match exactly one pin of the consumer record.
    pub owners: &'a [PathBuf],
}

/// One canonical process with the published consumer at image 0 and every pinned owner attached.
pub struct PublishedGlue {
    process: GlueProcess,
    packet: GlueArtifact,
    owners: Vec<(String, usize)>,
}

impl PublishedGlue {
    pub fn process(&self) -> &GlueProcess {
        &self.process
    }
    /// The consumer packet from the pinned record.
    pub fn packet(&self) -> &GlueArtifact {
        &self.packet
    }
    /// Process image index of the owner of `library`.
    pub fn owner_image(&self, library: &str) -> Option<usize> {
        self.owners.iter().find(|(owned, _)| owned == library).map(|(_, image)| *image)
    }
}

fn installed_driver_sha256(prefix: &Path) -> AotResult<String> {
    let path = prefix.join("bin").join(if cfg!(windows) {
        "beskid_native_tool_driver.exe"
    } else {
        "beskid_native_tool_driver"
    });
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| invalid(format!("installed compiler driver `{}`: {error}", path.display())))?;
    if !metadata.is_file() {
        return Err(invalid("installed compiler driver must be a regular file"));
    }
    hash_file(&path)
}

/// Admits a published consumer image and its Rust owner images into one new canonical process.
/// Every check runs on file bytes before any image is loaded; any difference fails closed.
pub fn admit_published_glue_images(req: &PublishedGlueRequest<'_>) -> AotResult<PublishedGlue> {
    if req.runtime.linkage != RuntimeLinkage::GlueSharedProviderV1 {
        return Err(invalid("published Glue admission requires the canonical shared provider profile"));
    }
    if !is_digest(req.consumer_sha256) {
        return Err(invalid("published Glue consumer pin must be a lowercase SHA-256 digest"));
    }
    let target = &req.runtime.kit.target;
    let triple = target.triple.as_str();
    let runtime = prepare_runtime(req.runtime)?;
    let provider_path = fs::canonicalize(
        runtime.shared_library_path.as_ref().ok_or_else(|| invalid("installed shared provider is missing"))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    let provider_sha256 = hash_file(&provider_path)?;
    let identity = provider_identity(triple, &provider_path)?;

    let consumer_path = fs::canonicalize(req.consumer).map_err(|error| invalid(error.to_string()))?;
    let consumer_bytes = bounded_file(&consumer_path)?;
    let consumer_sha256 = hash(&consumer_bytes);
    if consumer_sha256 != req.consumer_sha256 {
        return Err(invalid("published Glue consumer differs from its pinned digest"));
    }
    let native = target_shared_image(&consumer_bytes, target)?;
    let record: ConsumerRecord = read_record(&native, CONSUMER_RECORD_SYMBOL)?;
    let (packet, pins, mut producer) = record.into_witness(triple, &provider_sha256, &identity)?;
    require_exports(&native, &consumer_exports(&packet.manifest, &producer))?;
    require_sole_provider(&native, &identity)?;
    producer.native_sha256 = consumer_sha256.clone();

    if req.owners.len() != pins.len() {
        return Err(invalid(format!(
            "published Glue consumer pins {} owner images, but {} were supplied",
            pins.len(),
            req.owners.len()
        )));
    }
    let driver_sha256 = if pins.is_empty() { String::new() } else { installed_driver_sha256(&req.runtime.kit.prefix)? };
    let mut admitted = BTreeSet::new();
    let mut owners = Vec::with_capacity(pins.len());
    for path in req.owners {
        let path = fs::canonicalize(path).map_err(|error| invalid(error.to_string()))?;
        let bytes = bounded_file(&path)?;
        let digest = hash(&bytes);
        let pin = pins
            .iter()
            .find(|pin| pin.image_sha256 == digest)
            .ok_or_else(|| invalid(format!("owner image `{}` is not pinned by the consumer record", path.display())))?;
        if !admitted.insert(pin.library.clone()) {
            return Err(invalid("one pinned owner image was supplied twice"));
        }
        let native = target_shared_image(&bytes, target)?;
        let owner: OwnerRecord = read_record(&native, OWNER_RECORD_SYMBOL)?;
        if owner.format != RECORD_FORMAT || owner.compiler != RECORD_COMPILER {
            return Err(invalid(format!("owner `{}` was produced by `{}`, not `{RECORD_COMPILER}`", pin.library, owner.compiler)));
        }
        if owner.library != pin.library || owner.generation != producer.syntax_generation.0 {
            return Err(invalid("owner admission record names another library or source generation"));
        }
        if owner.provider_sha256 != provider_sha256 || owner.provider_identity != identity {
            return Err(invalid("owner image was built against a different runtime provider"));
        }
        if owner.driver_sha256 != driver_sha256 {
            return Err(invalid("owner image was built by a compiler driver other than the installed one"));
        }
        for tool in [&owner.cargo_sha256, &owner.rustc_sha256, &owner.linker_sha256] {
            if !is_digest(tool) {
                return Err(invalid("owner admission record tool digest is malformed"));
            }
        }
        if owner.source_inventory.is_empty() || owner.exports.len() > 4096 || owner.destructors.len() > 1024 {
            return Err(invalid("owner admission record closure is empty or out of bounds"));
        }
        if !owner.exports.iter().any(|symbol| symbol == OWNER_RECORD_SYMBOL)
            || owner.destructors.iter().any(|(_, symbol)| !owner.exports.contains(symbol))
            || owner.checked_exports.iter().any(|(_, symbol, _)| !owner.exports.contains(symbol))
        {
            return Err(invalid("owner admission record names a symbol outside its export closure"));
        }
        require_exports(&native, &owner.exports)?;
        require_sole_provider(&native, &identity)?;
        let destructors = owner
            .destructors
            .iter()
            .map(|(brand, symbol)| -> AotResult<([u8; 32], String)> { Ok((digest_array(brand)?, symbol.clone())) })
            .collect::<AotResult<Vec<_>>>()?;
        owners.push(IssuedRustOwner {
            library: owner.library,
            generation: owner.generation,
            payload_path: path,
            payload_sha256: digest,
            provider_path: provider_path.clone(),
            provider_sha256: provider_sha256.clone(),
            source_sha256: digest_array(&owner.source_sha256)?,
            destructors,
            checked_exports: owner.checked_exports,
            producer: None,
        });
    }

    let issued = IssuedGlueImage {
        packet: packet.clone(),
        payload_path: consumer_path,
        payload_sha256: consumer_sha256,
        provider_path,
        provider_sha256,
        producer,
    };
    let mut process = GlueProcess::open_issued(issued)?;
    let mut images = Vec::with_capacity(owners.len());
    for owner in owners {
        let library = owner.library.clone();
        let image = process.attach_issued_owner(owner)?;
        images.push((library, image));
    }
    Ok(PublishedGlue { process, packet, owners: images })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_hex_and_digest_fields_fail_closed() {
        assert_eq!(unhex("00ff10", 8).unwrap(), vec![0, 255, 16]);
        for bad in ["0", "0g", "FF", "00ff10"] {
            assert!(unhex(bad, 2).is_err(), "{bad} accepted");
        }
        assert!(digest_array(&"a".repeat(64)).is_ok());
        assert!(digest_array(&"A".repeat(64)).is_err());
        assert!(digest_array(&"a".repeat(62)).is_err());
    }

    #[test]
    fn framing_prefixes_magic_and_length() {
        let pin = OwnerPin { library: "glue_manual".into(), image_sha256: "a".repeat(64) };
        let framed = frame(&pin).unwrap();
        assert_eq!(&framed[..16], RECORD_MAGIC);
        let length = u64::from_le_bytes(framed[16..24].try_into().unwrap()) as usize;
        assert_eq!(length, framed.len() - 24);
        let decoded: OwnerPin = serde_json::from_slice(&framed[24..]).unwrap();
        assert_eq!(decoded, pin);
        let c = record_c_source("beskid_glue_artifact_v1_admission_record", &framed[..2]);
        assert!(c.contains("const uint8_t beskid_glue_artifact_v1_admission_record[2]={66,69,};"), "{c}");
        let rust = record_rust_source("beskid_glue_rust_owner_v1_admission_record", &framed[..2]);
        assert!(rust.contains("[u8; 2] = *b\"\\x42\\x45\";"), "{rust}");
    }
}
