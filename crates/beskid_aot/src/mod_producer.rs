//! Identity of the compiler that produces a native Mod artifact.
//!
//! The producer is the running process. Its identity is captured once, at the start of a Mod
//! build, from the running image itself: on Linux through `/proc/self/exe`, which names the
//! mapped image even after its path was replaced or removed; on macOS and Windows by opening the
//! executable path at capture time. The digest is taken from the open handle, so the evidence
//! staged into the artifact is the exact image that produced it.
//!
//! The artifact records this digest, and non-building consumers later qualify it against the
//! executable at the recorded path. A build whose on-disk executable changes (a toolchain
//! update while `beskid build` runs) can therefore never publish a qualifiable artifact; it fails
//! with an error that says so instead of an unrelated file error after linking.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{AotError, AotResult};

/// The producing compiler: its canonical executable path, the open running image and its digest.
pub struct ModProducerIdentity {
    executable: PathBuf,
    image: File,
    sha256: String,
}

impl ModProducerIdentity {
    /// Capture the identity of the running executable. Fails when the executable cannot be
    /// located or opened, or (Linux) when its path no longer names the running image.
    pub fn capture() -> AotResult<Self> {
        let located = std::env::current_exe()
            .map_err(|error| invalid(format!("locate the running Mod producer executable: {error}")))?;
        let executable = located.canonicalize().map_err(|error| {
            invalid(format!(
                "the running Mod producer executable {} is no longer on disk ({error}); it was replaced or removed \
                 after this process started, so rerun the command with the installed `beskid` CLI",
                located.display()
            ))
        })?;
        let mut image = open_running_image(&executable)?;
        let sha256 = handle_sha256(&mut image, &executable)?;
        let identity = Self { executable, image, sha256 };
        #[cfg(target_os = "linux")]
        identity.require_path_names_running_image()?;
        Ok(identity)
    }

    /// Canonical path of the producer executable; Mod workers and qualification use this path.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// SHA-256 of the running image, as captured.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The exact bytes of the running image, read from the handle opened at capture.
    pub fn image_bytes(&self) -> AotResult<Vec<u8>> {
        let mut image = self.image.try_clone().map_err(|error| io(&self.executable, error))?;
        image.seek(SeekFrom::Start(0)).map_err(|error| io(&self.executable, error))?;
        let mut bytes = Vec::new();
        image.read_to_end(&mut bytes).map_err(|error| io(&self.executable, error))?;
        if hex_sha256(&bytes) != self.sha256 {
            return Err(invalid(format!(
                "the running Mod producer image {} changed while it was open",
                self.executable.display()
            )));
        }
        Ok(bytes)
    }

    /// Require that the executable at [`Self::executable`] is still the captured image. `package`
    /// names the Mod being built for the error.
    pub fn require_unchanged_on_disk(&self, package: &str) -> AotResult<()> {
        let mut current = match File::open(&self.executable) {
            Ok(file) => file,
            Err(error) => {
                return Err(invalid(format!(
                    "the Mod producer executable {} was removed while building native Mod `{package}` ({error}); \
                     an artifact built by it could never qualify against the installed toolchain, so rerun the \
                     build with the installed `beskid` CLI",
                    self.executable.display()
                )));
            }
        };
        let current_sha256 = handle_sha256(&mut current, &self.executable)?;
        if current_sha256 != self.sha256 {
            return Err(invalid(format!(
                "the Mod producer executable {} was replaced while building native Mod `{package}` (running image \
                 sha256 {}, on-disk sha256 {current_sha256}); an artifact built by the old image could never \
                 qualify against the new executable, so rerun the build with the installed `beskid` CLI",
                self.executable.display(),
                self.sha256
            )));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn require_path_names_running_image(&self) -> AotResult<()> {
        use std::os::unix::fs::MetadataExt;
        let running = self.image.metadata().map_err(|error| io(&self.executable, error))?;
        let on_disk = std::fs::metadata(&self.executable).map_err(|error| io(&self.executable, error))?;
        if (running.dev(), running.ino()) != (on_disk.dev(), on_disk.ino()) {
            return Err(invalid(format!(
                "the Mod producer executable {} was replaced after this process started; rerun the command with \
                 the installed `beskid` CLI",
                self.executable.display()
            )));
        }
        Ok(())
    }
}

/// The running image: `/proc/self/exe` on Linux, else the executable path opened now.
fn open_running_image(executable: &Path) -> AotResult<File> {
    #[cfg(target_os = "linux")]
    let opened = File::open("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let opened = File::open(executable);
    opened.map_err(|error| invalid(format!("open the running Mod producer image {}: {error}", executable.display())))
}

fn handle_sha256(file: &mut File, path: &Path) -> AotResult<String> {
    file.seek(SeekFrom::Start(0)).map_err(|error| io(path, error))?;
    let mut hash = Sha256::new();
    std::io::copy(file, &mut hash).map_err(|error| io(path, error))?;
    Ok(format!("{:x}", hash.finalize()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: String) -> AotError {
    AotError::InvalidRequest { message }
}

fn io(path: &Path, error: impl std::fmt::Display) -> AotError {
    AotError::Io { path: path.to_path_buf(), message: error.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_the_running_image_and_reads_it_back_exactly() {
        let identity = ModProducerIdentity::capture().expect("capture producer identity");
        assert!(identity.executable().is_absolute());
        let bytes = identity.image_bytes().expect("image bytes");
        assert_eq!(hex_sha256(&bytes), identity.sha256());
        assert_eq!(hex_sha256(&std::fs::read(identity.executable()).expect("on-disk image")), identity.sha256());
        identity.require_unchanged_on_disk("Demo").expect("unchanged executable");
    }

    #[test]
    fn replaced_or_removed_executable_fails_clearly() {
        let directory = tempfile::tempdir().expect("directory");
        let executable = directory.path().join("beskid");
        std::fs::write(&executable, b"producer image v1").expect("executable");
        let mut image = File::open(&executable).expect("open image");
        let sha256 = handle_sha256(&mut image, &executable).expect("digest");
        let identity = ModProducerIdentity { executable: executable.clone(), image, sha256 };
        identity.require_unchanged_on_disk("Demo").expect("unchanged");

        // Replace the path with a new file, as an installer does (write a sibling, then rename).
        let staged = directory.path().join("beskid.new");
        std::fs::write(&staged, b"producer image v2").expect("new image");
        std::fs::rename(&staged, &executable).expect("replace executable");
        let error = identity.require_unchanged_on_disk("Demo").expect_err("replaced executable").to_string();
        assert!(error.contains("was replaced while building native Mod `Demo`"), "{error}");
        // The evidence still comes from the captured image, not from the replacement.
        assert_eq!(identity.image_bytes().expect("captured image"), b"producer image v1");

        std::fs::remove_file(&executable).expect("remove executable");
        let error = identity.require_unchanged_on_disk("Demo").expect_err("removed executable").to_string();
        assert!(error.contains("was removed while building native Mod `Demo`"), "{error}");
    }
}
