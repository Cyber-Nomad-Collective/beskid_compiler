//! Integrity checks for the Corelib snapshot shipped with the compiler.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const CORELIB_BUNDLE_FINGERPRINT_FILE: &str = ".beskid-bundle.sha256";

/// Return the nearest managed Corelib bundle root containing `path`: the closest physical
/// ancestor that holds a regular `.beskid-bundle.sha256` marker file.
///
/// This locates the bundle layout only; it does not verify the marker. Bundle integrity is an
/// install concern (`beskid_tools` re-materializes a bundle whose fingerprint differs) and never
/// grants Corelib service authority, which is decided per service file against the
/// compiler-embedded canonical sources.
pub fn corelib_bundle_marker_root(path: &Path) -> Option<PathBuf> {
    bundle_root_with_marker(path)
}

fn bundle_root_with_marker(path: &Path) -> Option<PathBuf> {
    let physical_path = path.canonicalize().ok()?;
    for candidate in path.ancestors() {
        let physical_root = candidate.canonicalize().ok()?;
        if !physical_path.starts_with(&physical_root) {
            continue;
        }
        let marker = physical_root.join(CORELIB_BUNDLE_FINGERPRINT_FILE);
        let Ok(metadata) = std::fs::symlink_metadata(&marker) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        return Some(physical_root);
    }
    None
}

/// Hash a Corelib bundle using the same stable file inventory as the embedded bundle builder.
pub fn fingerprint_corelib_bundle_dir(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_fingerprint_files(root, root, &mut files)?;
    files.sort();

    let mut digest = Sha256::new();
    for relative in files {
        let path_bytes = relative.to_string_lossy();
        digest.update((path_bytes.len() as u64).to_le_bytes());
        digest.update(path_bytes.as_bytes());

        let mut file = File::open(root.join(&relative))?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }

    let mut output = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut output, "{byte:02x}").expect("write fingerprint hex");
    }
    Ok(output)
}

fn collect_fingerprint_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name();
        if should_skip_component(&name) || name == CORELIB_BUNDLE_FINGERPRINT_FILE {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Corelib bundle contains a symlink: {}", entry.path().display()),
            ));
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_fingerprint_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root).expect("fingerprint path remains under root").to_path_buf());
        }
    }
    Ok(())
}

pub(crate) fn should_skip_component(name: &OsStr) -> bool {
    matches!(
        name,
        n if n == ".git"
            || n == "Project.lock"
            || n == "obj"
            || n == ".beskid"
            || n == "target"
            || n == ".venv-ci"
            || n == ".nox"
    )
}

#[cfg(test)]
mod bundle_inventory_tests {
    use std::ffi::OsStr;

    use super::should_skip_component;

    #[test]
    fn generated_project_lockfiles_are_not_embedded_in_release_corelib() {
        assert!(should_skip_component(OsStr::new("Project.lock")));
    }
}
