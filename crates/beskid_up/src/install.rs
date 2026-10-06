//! Immutable verified complete-prefix storage and atomic activation.
#![allow(non_snake_case)]
use crate::{ReleaseManifest, UpError};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const RECEIPT: &str = ".beskid-install.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallReceipt {
    schema: u32,
    version: String,
    commit: String,
    target: String,
    artifact_sha256: String,
    files: std::collections::BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct DirectInstall {
    root: PathBuf,
}
impl DirectInstall {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_owned() }
    }
    /// Install exact downloaded archive bytes; neither activation nor old payload mutation occurs here.
    pub fn InstallArchive(&self, manifest: &ReleaseManifest, target: &str, archive: &[u8]) -> Result<PathBuf, UpError> {
        let bundle = manifest.select_bundle(target)?;
        if archive.len() as u64 > MAX_ARCHIVE_BYTES || Hash(archive) != bundle.sha256.to_ascii_lowercase() {
            return Err(Invalid("bundle checksum or compressed-size limit failed"));
        }
        for path in [&self.root, &self.root.join("versions")] {
            if let Ok(metadata) = fs::symlink_metadata(path) {
                if !metadata.file_type().is_dir() {
                    return Err(Invalid("direct-install storage must be a regular directory"));
                }
            }
        }
        fs::create_dir_all(self.root.join("versions")).map_err(Io)?;
        let stage =
            tempfile::Builder::new().prefix(".beskid-install-").tempdir_in(self.root.join("versions")).map_err(Io)?;
        let prefix_name = format!("beskid-{}-{target}", manifest.version);
        Extract(archive, stage.path(), &prefix_name)?;
        ValidatePrefix(stage.path(), &manifest.version, target)?;
        crate::owner::WriteOwnerReceipt(stage.path(), crate::InstallationOwner::Direct, &manifest.version, target)?;
        let receipt = InstallReceipt {
            schema: 1,
            version: manifest.version.to_string(),
            commit: manifest.commit.clone(),
            target: target.into(),
            artifact_sha256: bundle.sha256.to_ascii_lowercase(),
            files: Inventory(stage.path())?,
        };
        let mut file =
            fs::OpenOptions::new().create_new(true).write(true).open(stage.path().join(RECEIPT)).map_err(Io)?;
        file.write_all(&serde_json::to_vec(&receipt).map_err(|error| Invalid(&error.to_string()))?).map_err(Io)?;
        file.sync_all().map_err(Io)?;
        let destination = self.version_dir(&manifest.version);
        if destination.exists() {
            ValidateInstalled(&destination, &manifest.version)?;
            let existing: InstallReceipt = serde_json::from_slice(&fs::read(destination.join(RECEIPT)).map_err(Io)?)
                .map_err(|error| Invalid(&error.to_string()))?;
            if existing.commit != receipt.commit
                || existing.artifact_sha256 != receipt.artifact_sha256
                || existing.files != receipt.files
            {
                return Err(Invalid("immutable installed coordinate differs from requested bundle"));
            }
            return Ok(destination);
        }
        fs::rename(stage.path(), &destination).map_err(Io)?;
        SyncDirectory(&self.root.join("versions"))?;
        Ok(destination)
    }
    pub fn activate(&self, version: &Version) -> Result<(), UpError> {
        ValidateInstalled(&self.version_dir(version), version)?;
        fs::create_dir_all(&self.root).map_err(Io)?;
        if let Ok(metadata) = fs::symlink_metadata(self.root.join("active")) {
            if !metadata.file_type().is_file() {
                return Err(Invalid("active pointer is not a regular file; existing path was preserved"));
            }
        }
        let pending = tempfile::NamedTempFile::new_in(&self.root).map_err(Io)?;
        fs::write(pending.path(), version.to_string()).map_err(Io)?;
        pending.as_file().sync_all().map_err(Io)?;
        // Persist uses the platform's atomic replacement primitive; a failure leaves old active untouched.
        pending.persist(self.root.join("active")).map_err(|error| Io(error.error))?;
        SyncDirectory(&self.root)
    }
    pub fn active_version(&self) -> Result<Option<Version>, UpError> {
        let active = self.root.join("active");
        if !active.exists() {
            return Ok(None);
        }
        if !fs::symlink_metadata(&active).map_err(Io)?.file_type().is_file() {
            return Err(Invalid("active pointer is not a regular file"));
        }
        let version = Version::parse(fs::read_to_string(active).map_err(Io)?.trim())
            .map_err(|error| Invalid(&error.to_string()))?;
        ValidateInstalled(&self.version_dir(&version), &version)?;
        Ok(Some(version))
    }
    pub fn ActivePrefix(&self) -> Result<Option<PathBuf>, UpError> {
        Ok(self.active_version()?.map(|version| self.version_dir(&version)))
    }
    pub fn remove(&self, version: &Version) -> Result<(), UpError> {
        if self.active_version()?.as_ref() == Some(version) {
            return Err(Invalid("select a different version before removing active toolchain"));
        }
        ValidateInstalled(&self.version_dir(version), version)?;
        fs::remove_dir_all(self.version_dir(version)).map_err(Io)
    }
    fn version_dir(&self, version: &Version) -> PathBuf {
        self.root.join("versions").join(version.to_string())
    }
}
pub(crate) fn ValidateInstalled(prefix: &Path, version: &Version) -> Result<(), UpError> {
    if !fs::symlink_metadata(prefix).map_err(Io)?.file_type().is_dir() {
        return Err(Invalid("installed prefix is not a regular directory"));
    }
    let bytes = fs::read(prefix.join(RECEIPT)).map_err(Io)?;
    let receipt: InstallReceipt = serde_json::from_slice(&bytes).map_err(|error| Invalid(&error.to_string()))?;
    if receipt.schema != 1
        || receipt.version != version.to_string()
        || receipt.commit.len() != 40
        || receipt.artifact_sha256.len() != 64
    {
        return Err(Invalid("installed identity receipt does not match version/source/artifact"));
    }
    ValidatePrefix(prefix, version, &receipt.target)?;
    if Inventory(prefix)? != receipt.files {
        return Err(Invalid("installed toolchain bytes or file inventory changed"));
    }
    crate::owner::ValidateDirectOwner(prefix, version, &receipt.target)?;
    Ok(())
}
pub(crate) fn ValidatePrefix(prefix: &Path, version: &Version, target: &str) -> Result<(), UpError> {
    let target_metadata = beskid_abi::abi_v5::TargetMetadata::for_triple(target)
        .map_err(|_| UpError::UnsupportedTarget(target.into()))?;
    if fs::read(prefix.join("release-version.txt")).map_err(Io)? != format!("{version}\n").as_bytes() {
        return Err(Invalid("bundle release version differs"));
    }
    let extension = if target.ends_with("windows-msvc") { ".exe" } else { "" };
    for tool in ["beskid", "beskid_lsp", "beskid-up"] {
        let path = prefix.join("bin").join(format!("{tool}{extension}"));
        let metadata = fs::symlink_metadata(path).map_err(Io)?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(Invalid("bundle tool absent or not a regular nonempty file"));
        }
        #[cfg(unix)]
        if extension.is_empty() {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(Invalid("bundle tool is not executable"));
            }
        }
    }
    let corelib = prefix.join("beskid_corelib");
    for path in ["CoreLib.bws", "beskid_corelib/corelib.bproj"] {
        if !corelib.join(path).is_file() {
            return Err(Invalid("Corelib manifests missing"));
        }
    }
    if !corelib.join("packages").is_dir()
        || beskid_abi::corelib_bundle::verified_corelib_bundle_root(&corelib).is_none()
    {
        return Err(Invalid("Corelib bundle fingerprint invalid"));
    }
    for profile in [beskid_abi::runtime_kit::BuildProfile::Debug, beskid_abi::runtime_kit::BuildProfile::Release] {
        beskid_abi::runtime_kit::resolve_installed_runtime_kit(prefix, &target_metadata, profile)
            .map_err(|error| Invalid(&format!("runtime kit invalid: {error:?}")))?;
    }
    Ok(())
}
fn Extract(bytes: &[u8], destination: &Path, root: &str) -> Result<(), UpError> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut seen = std::collections::HashSet::new();
    let mut expanded = 0u64;
    for (index, entry) in archive.entries().map_err(Io)?.enumerate() {
        if index >= 100_000 {
            return Err(Invalid("bundle has too many entries"));
        }
        let mut entry = entry.map_err(Io)?;
        let path = entry.path().map_err(Io)?.into_owned();
        if path.as_os_str().len() > 4096
            || !path.components().all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(Invalid("unsafe bundle entry path"));
        }
        let relative = path.strip_prefix(root).map_err(|_| Invalid("bundle root differs from version/target"))?;
        let identity = relative.to_string_lossy().to_lowercase();
        if !seen.insert(identity) {
            return Err(Invalid("duplicate or case-conflicting bundle path"));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(Invalid("bundle links and special entries are unsupported"));
        }
        expanded = expanded.checked_add(entry.size()).ok_or_else(|| Invalid("bundle size overflow"))?;
        if expanded > MAX_EXPANDED_BYTES {
            return Err(Invalid("bundle expanded size limit exceeded"));
        }
        if relative.as_os_str().is_empty() {
            if !kind.is_dir() {
                return Err(Invalid("bundle root is not a directory"));
            }
            continue;
        }
        let output = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(output).map_err(Io)?;
        } else {
            fs::create_dir_all(output.parent().ok_or_else(|| Invalid("bundle entry has no parent"))?).map_err(Io)?;
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&output).map_err(Io)?;
            io::copy(&mut entry, &mut file).map_err(Io)?;
            file.sync_all().map_err(Io)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = entry.header().mode().map_err(Io)? & 0o777;
                fs::set_permissions(output, fs::Permissions::from_mode(mode)).map_err(Io)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn Inventory(prefix: &Path) -> Result<std::collections::BTreeMap<String, String>, UpError> {
    fn Visit(
        root: &Path,
        current: &Path,
        files: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), UpError> {
        for entry in fs::read_dir(current).map_err(Io)? {
            let entry = entry.map_err(Io)?;
            let kind = entry.file_type().map_err(Io)?;
            if kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
                return Err(Invalid("toolchain has a link or special file"));
            }
            if kind.is_dir() {
                Visit(root, &entry.path(), files)?;
            } else if entry.path() != root.join(RECEIPT) && entry.path() != root.join(crate::owner::OWNER_RECEIPT) {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| Invalid("inventory escape"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(relative, Hash(&fs::read(entry.path()).map_err(Io)?));
            }
        }
        Ok(())
    }
    let mut files = std::collections::BTreeMap::new();
    Visit(prefix, prefix, &mut files)?;
    Ok(files)
}
fn Hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn Invalid(message: &str) -> UpError {
    UpError::InvalidManifest(message.into())
}
fn Io(error: io::Error) -> UpError {
    Invalid(&error.to_string())
}
fn SyncDirectory(path: &Path) -> Result<(), UpError> {
    #[cfg(unix)]
    {
        fs::File::open(path).map_err(Io)?.sync_all().map_err(Io)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Download a bounded source-bound manifest and bundle, verify, install, then atomically select it.
pub fn UpdateConfiguredToolchain() -> Result<PathBuf, UpError> {
    let installation = crate::CurrentInstallationStatus()?;
    if !installation.direct_update_allowed {
        return Err(Invalid(&format!(
            "installation owner cannot perform direct update: {}",
            installation.update_guidance
        )));
    }
    let manifest_url = std::env::var("BESKID_RELEASE_MANIFEST_URL")
        .map_err(|_| Invalid("set BESKID_RELEASE_MANIFEST_URL to the qualified immutable release manifest"))?;
    if !manifest_url.starts_with("https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v")
        || manifest_url.contains('?')
        || manifest_url.contains('#')
    {
        return Err(Invalid("release manifest must use the immutable official HTTPS release origin"));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| Invalid(&error.to_string()))?;
    let mut response = client
        .get(&manifest_url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| Invalid("release manifest request failed"))?;
    let mut manifest_bytes = Vec::new();
    response.by_ref().take(1024 * 1024 + 1).read_to_end(&mut manifest_bytes).map_err(Io)?;
    if manifest_bytes.len() > 1024 * 1024 {
        return Err(Invalid("release manifest exceeds size limit"));
    }
    let manifest = ReleaseManifest::from_json(
        std::str::from_utf8(&manifest_bytes).map_err(|_| Invalid("release manifest is not UTF-8"))?,
    )?;
    let expected_origin = format!(
        "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v{}/",
        manifest.version
    );
    if !manifest_url.starts_with(&expected_origin) || manifest_url.contains('?') || manifest_url.contains('#') {
        return Err(Invalid("manifest URL does not match its immutable release version"));
    }
    let target = beskid_abi::runtime_kit::host_runtime_triple().map_err(|error| Invalid(&error.to_string()))?;
    let bundle = manifest.select_bundle(target)?;
    let mut response = client
        .get(&bundle.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| Invalid(&error.to_string()))?;
    let mut bytes = Vec::new();
    response.by_ref().take(MAX_ARCHIVE_BYTES + 1).read_to_end(&mut bytes).map_err(Io)?;
    let store = crate::owner::ConfiguredStore()?;
    let prefix = store.InstallArchive(&manifest, target, &bytes)?;
    store.activate(&manifest.version)?;
    println!(
        "Verified beskid {} from {}. Active prefix: {}. Use its bin directory for subsequent launches.",
        manifest.version,
        manifest.commit,
        prefix.display()
    );
    Ok(prefix)
}
