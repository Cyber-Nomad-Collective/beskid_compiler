//! Immutable SDK source proof embedded into this compiler, independent of installed paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkSourceError(&'static str);
impl std::fmt::Display for SdkSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for SdkSourceError {}
use sha2::{Digest, Sha256};

pub struct CanonicalSdkSource {
    path: &'static str,
    bytes: &'static [u8],
}
impl CanonicalSdkSource {
    pub fn path(&self) -> &'static str {
        self.path
    }
    pub fn bytes(&self) -> &'static [u8] {
        self.bytes
    }
}
include!(concat!(env!("OUT_DIR"), "/canonical_sdk_sources.rs"));
pub fn canonical_sdk_sources() -> &'static [CanonicalSdkSource] {
    CANONICAL_SDK_SOURCES
}

/// This is a source-correspondence check, not a constructor for callback authority.
/// Callback issuance additionally requires current registered package, generation and layouts.
pub fn verify_canonical_sdk_source(path: &str, bytes: &[u8]) -> Result<&'static CanonicalSdkSource, SdkSourceError> {
    let index = CANONICAL_SDK_SOURCES
        .binary_search_by_key(&path, |source| source.path)
        .map_err(|_| SdkSourceError("source is not part of this compiler's canonical SDK"))?;
    let source = &CANONICAL_SDK_SOURCES[index];
    if bytes != source.bytes {
        return Err(SdkSourceError("SDK source differs from this compiler; rebuild the SDK and Mod together"));
    }
    Ok(source)
}
/// Framed, platform-independent identity of the exact compiled SDK source closure.
pub fn canonical_sdk_source_digest() -> String {
    let mut digest = Sha256::new();
    digest.update(b"beskid.canonical-sdk-sources.v2\0");
    digest.update((CANONICAL_SDK_SOURCES.len() as u64).to_le_bytes());
    for source in CANONICAL_SDK_SOURCES {
        digest.update((source.path.len() as u64).to_le_bytes());
        digest.update(source.path.as_bytes());
        digest.update((source.bytes.len() as u64).to_le_bytes());
        digest.update(source.bytes);
    }
    format!("{:x}", digest.finalize())
}
