use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::errors::ArtifactError;

pub(crate) const MAX_ENTRIES: usize = 10_000;
pub(crate) const MAX_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
/// Browsing is deliberately capped well below the accepted artifact size.
/// Consumers render individual files, they never need a whole source tree in
/// one response.
pub const MAX_BROWSE_READ_BYTES: u64 = 1024 * 1024;
pub(crate) const REQUIRED_ENTRIES: [&str; 2] = ["package.json", "checksums.sha256"];

pub(crate) fn parse_checksums(contents: &str) -> Result<std::collections::BTreeMap<String, String>, ArtifactError> {
    let mut checksums = std::collections::BTreeMap::new();
    for line in contents.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let mut chunks = line.split_whitespace();
        let digest = chunks.next().ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        let path =
            chunks.next_back().ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        if chunks.next().is_some() || !is_sha256(digest) {
            return Err(ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")));
        }
        checksums.insert(normalize_zip_path(path)?, digest.to_ascii_lowercase());
    }
    Ok(checksums)
}

pub(crate) fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<String, ArtifactError> {
    String::from_utf8(read_entry_bytes(zip, index)?)
        .map_err(|_| ArtifactError::InvalidZip("text entry is not UTF-8".into()))
}

pub(crate) fn read_entry_bytes(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<Vec<u8>, ArtifactError> {
    let mut bytes = Vec::new();
    zip.by_index(index)
        .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?
        .read_to_end(&mut bytes)
        .map_err(crate::errors::io_error)?;
    Ok(bytes)
}

pub(crate) fn read_entry_limited(
    zip: &mut ZipArchive<Cursor<&[u8]>>,
    index: usize,
    path: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let entry = zip.by_index(index).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
    let mut bytes = Vec::with_capacity(entry.size().min(MAX_BROWSE_READ_BYTES) as usize);
    entry.take(MAX_BROWSE_READ_BYTES + 1).read_to_end(&mut bytes).map_err(crate::errors::io_error)?;
    if bytes.len() as u64 > MAX_BROWSE_READ_BYTES {
        return Err(ArtifactError::EntryTooLarge { path: path.to_owned(), limit_bytes: MAX_BROWSE_READ_BYTES });
    }
    Ok(bytes)
}

pub(crate) fn normalize_zip_path(path: &str) -> Result<String, ArtifactError> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ArtifactError::InvalidZip(format!("unsafe entry path '{path}'")));
    }
    Ok(path)
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
