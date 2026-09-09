use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub(crate) const BUNDLE_FINGERPRINT_FILE: &str = ".beskid-bundle.sha256";

pub(crate) fn fingerprint_dir(root: &Path) -> std::io::Result<String> {
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

    let bytes = digest.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("write fingerprint hex");
    }
    Ok(output)
}

fn collect_fingerprint_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if should_skip_component(&entry.file_name()) {
            continue;
        }
        if path.is_dir() {
            collect_fingerprint_files(root, &path, files)?;
        } else if entry.file_name() != BUNDLE_FINGERPRINT_FILE {
            files.push(path.strip_prefix(root).expect("fingerprint path remains under root").to_path_buf());
        }
    }
    Ok(())
}

pub(crate) fn should_skip_component(name: &OsStr) -> bool {
    matches!(
        name,
        n if n == ".git"
            || n == "obj"
            || n == ".beskid"
            || n == "target"
            || n == ".venv-ci"
            || n == ".nox"
    )
}
