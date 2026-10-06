//! Installation ownership is derived from the running executable's complete prefix.
#![allow(non_snake_case)]
use crate::UpError;
use crate::install::{DirectInstall, Inventory, ValidateInstalled, ValidatePrefix};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) const OWNER_RECEIPT: &str = ".beskid-owner.json";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationOwner {
    Direct,
    Homebrew,
    Debian,
    MacosInstaller,
    WindowsInstaller,
    Manual,
    Container,
}
impl InstallationOwner {
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Homebrew => "homebrew",
            Self::Debian => "debian",
            Self::MacosInstaller => "macos-installer",
            Self::WindowsInstaller => "windows-installer",
            Self::Manual => "manual",
            Self::Container => "container",
        }
    }
    pub fn guidance(self) -> &'static str {
        match self {
            Self::Direct => "Use beskid toolchain update from the active verified direct installation.",
            Self::Homebrew => "Update through the owning package manager: brew upgrade beskid",
            Self::Debian => {
                "Download the qualified replacement DEB, then install through its owner: sudo apt install ./beskid-<version>-amd64.deb"
            }
            Self::MacosInstaller => {
                "Install the qualified replacement Beskid.app from the official macOS distribution."
            }
            Self::WindowsInstaller => "Install the qualified replacement MSI through Windows Installer.",
            Self::Manual => {
                "Replace this private toolchain prefix with the qualified complete bundle using its installer."
            }
            Self::Container => "Rebuild or pull the qualified container image through its deployment owner.",
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OwnerReceipt {
    schema: u32,
    owner: InstallationOwner,
    version: String,
    target: String,
    artifact_identity: String,
    files: BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct InstallationStatus {
    pub executable: PathBuf,
    pub prefix: Option<PathBuf>,
    pub owner: Option<InstallationOwner>,
    pub version: Option<Version>,
    pub target: Option<String>,
    pub direct_update_allowed: bool,
    pub update_guidance: String,
}
impl std::fmt::Display for InstallationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Executable: {}", self.executable.display())?;
        writeln!(f, "Installation owner: {}", self.owner.map(InstallationOwner::name).unwrap_or("unmanaged"))?;
        if let Some(prefix) = &self.prefix {
            writeln!(f, "Prefix: {}", prefix.display())?;
        }
        if let Some(version) = &self.version {
            writeln!(f, "Version: {version}")?;
        }
        if let Some(target) = &self.target {
            writeln!(f, "Target: {target}")?;
        }
        write!(f, "{}", self.update_guidance)
    }
}
fn Invalid(message: impl std::fmt::Display) -> UpError {
    UpError::InvalidManifest(format!("installation owner: {message}; reinstall through the owning channel"))
}
fn ArtifactIdentity(files: &BTreeMap<String, String>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"beskid-installation-inventory-v1\0");
    for (path, digest) in files {
        hash.update((path.len() as u64).to_be_bytes());
        hash.update(path.as_bytes());
        hash.update((digest.len() as u64).to_be_bytes());
        hash.update(digest.as_bytes());
    }
    format!("sha256-beskid-inventory-v1:{hash:x}", hash = hash.finalize())
}
fn ReadReceipt(prefix: &Path, validate_direct: bool) -> Result<OwnerReceipt, UpError> {
    let metadata = fs::symlink_metadata(prefix.join(OWNER_RECEIPT)).map_err(Invalid)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(Invalid("receipt is not a bounded regular file"));
    }
    let receipt: OwnerReceipt =
        serde_json::from_slice(&fs::read(prefix.join(OWNER_RECEIPT)).map_err(Invalid)?).map_err(Invalid)?;
    let version = Version::parse(&receipt.version).map_err(Invalid)?;
    if receipt.schema != 1 {
        return Err(Invalid("unsupported receipt schema"));
    }
    ValidatePrefix(prefix, &version, &receipt.target)?;
    let files = Inventory(prefix)?;
    if files != receipt.files || ArtifactIdentity(&files) != receipt.artifact_identity {
        return Err(Invalid("complete payload inventory or artifact identity changed"));
    }
    if receipt.owner != InstallationOwner::Direct && prefix.join(".beskid-install.json").exists() {
        return Err(Invalid("conflicting direct and package owner receipts"));
    }
    if validate_direct && receipt.owner == InstallationOwner::Direct {
        ValidateInstalled(prefix, &version)?;
    }
    Ok(receipt)
}
/// Stamp a qualified complete private prefix before the installer publishes it.
pub fn WriteOwnerReceipt(
    prefix: &Path,
    owner: InstallationOwner,
    version: &Version,
    target: &str,
) -> Result<(), UpError> {
    ValidatePrefix(prefix, version, target)?;
    if prefix.join(OWNER_RECEIPT).exists() {
        let existing = ReadReceipt(prefix, true)?;
        if existing.owner != owner || existing.version != version.to_string() || existing.target != target {
            return Err(Invalid("existing receipt has a different owner or coordinate"));
        }
        return Ok(());
    }
    if owner != InstallationOwner::Direct && prefix.join(".beskid-install.json").exists() {
        return Err(Invalid("conflicting direct install receipt"));
    }
    let files = Inventory(prefix)?;
    let receipt = OwnerReceipt {
        schema: 1,
        owner,
        version: version.to_string(),
        target: target.into(),
        artifact_identity: ArtifactIdentity(&files),
        files,
    };
    use std::io::Write;
    let mut file =
        fs::OpenOptions::new().create_new(true).write(true).open(prefix.join(OWNER_RECEIPT)).map_err(Invalid)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt).map_err(Invalid)?).map_err(Invalid)?;
    file.sync_all().map_err(Invalid)?;
    Ok(())
}
/// Read ownership without provisioning or consulting runtime-prefix overrides.
pub fn InspectInstallation(executable: &Path, store: &DirectInstall) -> Result<InstallationStatus, UpError> {
    let executable = fs::canonicalize(executable).map_err(Invalid)?;
    let prefix = beskid_abi::runtime_kit::installed_toolchain_prefix_for_executable(&executable).ok();
    let mut status = InstallationStatus {
        executable,
        prefix,
        owner: None,
        version: None,
        target: None,
        direct_update_allowed: false,
        update_guidance:
            "Unmanaged installation owner: use a qualified channel installer before updating this executable.".into(),
    };
    let Some(prefix) = &status.prefix else {
        return Ok(status);
    };
    if !prefix.join(OWNER_RECEIPT).try_exists().map_err(Invalid)? {
        return Ok(status);
    }
    let receipt = ReadReceipt(prefix, true)?;
    status.owner = Some(receipt.owner);
    status.version = Some(Version::parse(&receipt.version).map_err(Invalid)?);
    status.target = Some(receipt.target);
    status.update_guidance = receipt.owner.guidance().into();
    if receipt.owner == InstallationOwner::Direct {
        status.direct_update_allowed =
            store.ActivePrefix()?.map(|active| fs::canonicalize(active).map_err(Invalid)).transpose()?.as_ref()
                == Some(prefix);
        if !status.direct_update_allowed {
            status.update_guidance = "Direct owner is inactive or belongs to a different store; launch the active verified installation before updating.".into();
        }
    }
    Ok(status)
}
pub(crate) fn ConfiguredStore() -> Result<DirectInstall, UpError> {
    let root = if let Some(root) = std::env::var_os("BESKID_HOME") {
        PathBuf::from(root)
    } else {
        PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or_else(|| Invalid("set BESKID_HOME to choose direct-install storage"))?,
        )
        .join(".local/share/beskid")
    };
    Ok(DirectInstall::new(root))
}
pub fn CurrentInstallationStatus() -> Result<InstallationStatus, UpError> {
    InspectInstallation(&std::env::current_exe().map_err(Invalid)?, &ConfiguredStore()?)
}

pub(crate) fn ValidateDirectOwner(prefix: &Path, version: &Version, target: &str) -> Result<(), UpError> {
    let receipt = ReadReceipt(prefix, false)?;
    if receipt.owner != InstallationOwner::Direct || receipt.version != version.to_string() || receipt.target != target
    {
        return Err(Invalid("direct-install receipt conflicts with owner receipt"));
    }
    Ok(())
}
