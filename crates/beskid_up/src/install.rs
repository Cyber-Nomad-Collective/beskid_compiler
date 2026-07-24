use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use semver::Version;

use crate::UpError;

/// Versioned payload storage for direct-download installations.
pub struct DirectInstall {
    root: PathBuf,
}

impl DirectInstall {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_owned() }
    }

    pub fn install_empty(&self, version: &Version) -> Result<(), UpError> {
        fs::create_dir_all(self.version_dir(version)).map_err(io_error)?;
        Ok(())
    }

    pub fn activate(&self, version: &Version) -> Result<(), UpError> {
        if !self.version_dir(version).is_dir() {
            return Err(UpError::InvalidManifest(format!("version {version} is not installed")));
        }
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let pending = self.root.join("active.pending");
        fs::write(&pending, version.to_string()).map_err(io_error)?;
        fs::rename(pending, self.root.join("active")).map_err(io_error)
    }

    pub fn active_version(&self) -> Result<Option<Version>, UpError> {
        let active = self.root.join("active");
        if !active.exists() {
            return Ok(None);
        }
        let value = fs::read_to_string(active).map_err(io_error)?;
        Version::parse(value.trim())
            .map(Some)
            .map_err(|error| UpError::InvalidManifest(format!("invalid active version: {error}")))
    }

    pub fn remove(&self, version: &Version) -> Result<(), UpError> {
        if self.active_version()?.as_ref() == Some(version) {
            return Err(UpError::InvalidManifest("select a different version before removing the active one".into()));
        }
        fs::remove_dir_all(self.version_dir(version)).map_err(io_error)
    }

    fn version_dir(&self, version: &Version) -> PathBuf {
        self.root.join("versions").join(version.to_string())
    }
}

fn io_error(error: io::Error) -> UpError {
    UpError::InvalidManifest(error.to_string())
}
