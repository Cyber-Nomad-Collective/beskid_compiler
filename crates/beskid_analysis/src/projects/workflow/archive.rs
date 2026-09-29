use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use tempfile::NamedTempFile;
use zip::ZipArchive;

use crate::projects::error::ProjectError;

const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchivePathKind {
    Directory,
    File,
}

pub(super) fn extract_zip_to_dir(file: File, output_dir: &Path) -> Result<(), ProjectError> {
    let mut archive = ZipArchive::new(file)
        .map_err(|err| ProjectError::Validation(format!("invalid registry artifact ZIP: {err}")))?;

    // Inspect all names and declared sizes before writing any entry. The copy
    // loop also counts actual output because ZIP metadata is untrusted.
    let mut planned_paths = BTreeMap::new();
    let mut declared_total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        let path = checked_entry_name(&entry)?;
        plan_archive_path(&mut planned_paths, &path, entry.is_dir())?;
        if !entry.is_dir() {
            check_output_limits(entry.size(), &mut declared_total)?;
        }
        validate_destination(output_dir, &path, entry.is_dir())?;
    }

    let mut actual_total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        let path = checked_entry_name(&entry)?;
        let target = validate_destination(output_dir, &path, entry.is_dir())?;
        if entry.is_dir() {
            create_checked_directories(output_dir, &path)?;
            continue;
        }

        let parent = path.parent().expect("checked ZIP entry has a parent");
        create_checked_directories(output_dir, parent)?;
        validate_destination(output_dir, &path, false)?;

        // Persisting a temporary file replaces an existing symlink itself,
        // rather than following it as File::create would. Parent paths are
        // checked again immediately before the replacement.
        let mut scratch = NamedTempFile::new_in(output_dir.join(parent)).map_err(|source| {
            ProjectError::MaterializationCopy { from: output_dir.to_path_buf(), to: target.clone(), source }
        })?;
        copy_entry_with_limits(&mut entry, scratch.as_file_mut(), &target, &mut actual_total)?;
        validate_destination(output_dir, &path, false)?;
        scratch.persist(&target).map_err(|error| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target,
            source: error.error,
        })?;
    }

    Ok(())
}

fn plan_archive_path(
    paths: &mut BTreeMap<PathBuf, ArchivePathKind>,
    path: &Path,
    is_dir: bool,
) -> Result<(), ProjectError> {
    let mut prefix = PathBuf::new();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        prefix.push(segment);
        let kind =
            if is_dir || index + 1 < components.len() { ArchivePathKind::Directory } else { ArchivePathKind::File };
        match paths.get(&prefix) {
            Some(existing) if *existing != kind || kind == ArchivePathKind::File => {
                return Err(ProjectError::Validation(format!(
                    "registry artifact ZIP entries conflict at {}",
                    prefix.display()
                )));
            }
            Some(_) => {}
            None => {
                paths.insert(prefix.clone(), kind);
            }
        }
    }
    Ok(())
}

fn check_output_limits(entry_bytes: u64, total: &mut u64) -> Result<(), ProjectError> {
    if entry_bytes > MAX_ENTRY_UNCOMPRESSED_BYTES {
        return Err(ProjectError::Validation(
            "registry artifact entry exceeds the 512 MiB uncompressed size limit".into(),
        ));
    }
    *total = total
        .checked_add(entry_bytes)
        .ok_or_else(|| ProjectError::Validation("registry artifact uncompressed size overflows its budget".into()))?;
    if *total > MAX_TOTAL_UNCOMPRESSED_BYTES {
        return Err(ProjectError::Validation(
            "registry artifact exceeds the 1 GiB total uncompressed size limit".into(),
        ));
    }
    Ok(())
}

fn copy_entry_with_limits(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &mut File,
    destination_path: &Path,
    total: &mut u64,
) -> Result<(), ProjectError> {
    let mut entry_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = entry.read(&mut buffer).map_err(|source| ProjectError::MaterializationCopy {
            from: destination_path.to_path_buf(),
            to: destination_path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        let read = read as u64;
        entry_bytes = entry_bytes.checked_add(read).ok_or_else(|| {
            ProjectError::Validation("registry artifact entry uncompressed size overflows its budget".into())
        })?;
        if entry_bytes > MAX_ENTRY_UNCOMPRESSED_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact entry exceeds the 512 MiB uncompressed size limit".into(),
            ));
        }
        *total = total.checked_add(read).ok_or_else(|| {
            ProjectError::Validation("registry artifact total uncompressed size overflows its budget".into())
        })?;
        if *total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact exceeds the 1 GiB total uncompressed size limit".into(),
            ));
        }
        destination.write_all(&buffer[..read as usize]).map_err(|source| ProjectError::MaterializationCopy {
            from: destination_path.to_path_buf(),
            to: destination_path.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn checked_entry_name(entry: &zip::read::ZipFile<'_>) -> Result<PathBuf, ProjectError> {
    let path = entry
        .enclosed_name()
        .ok_or_else(|| ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()))?;
    let mode_type = entry.unix_mode().map(|mode| mode & 0o170000).unwrap_or(0);
    if path.as_os_str().is_empty()
        || entry.is_symlink()
        || !matches!(mode_type, 0 | 0o040000 | 0o100000)
        || mode_type == 0o040000 && !entry.is_dir()
        || mode_type == 0o100000 && entry.is_dir()
    {
        return Err(ProjectError::Validation("registry artifact contains a symlink or special ZIP entry".into()));
    }
    Ok(path)
}

fn validate_destination(output_dir: &Path, relative: &Path, is_dir: bool) -> Result<PathBuf, ProjectError> {
    for ancestor in output_dir.ancestors() {
        validate_existing_path(ancestor, true, false)?;
    }

    let components = relative.components().collect::<Vec<_>>();
    let mut target = output_dir.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        target.push(segment);
        validate_existing_path(&target, is_dir || index + 1 < components.len(), true)?;
    }
    Ok(target)
}

fn validate_existing_path(path: &Path, is_dir: bool, allow_missing: bool) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_redirecting_path(&metadata) => Err(ProjectError::Validation(format!(
            "registry artifact extraction path contains a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if is_dir && !metadata.is_dir() || !is_dir && !metadata.is_file() => {
            Err(ProjectError::Validation(format!(
                "registry artifact extraction path has the wrong file type: {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if allow_missing && error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ProjectError::Validation(format!(
            "registry artifact extraction path cannot be inspected: {}: {error}",
            path.display()
        ))),
    }
}

fn is_redirecting_path(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

pub(super) fn verify_materialized_tree(expected: &Path, actual: &Path) -> Result<(), ProjectError> {
    let expected_metadata = fs::symlink_metadata(expected)
        .map_err(|error| tampered_materialization(actual, &format!("staged package cannot be inspected: {error}")))?;
    let actual_metadata = fs::symlink_metadata(actual)
        .map_err(|error| tampered_materialization(actual, &format!("cached package cannot be inspected: {error}")))?;
    if is_redirecting_path(&actual_metadata) {
        return Err(tampered_materialization(actual, "contains a symlink or reparse point"));
    }
    if expected_metadata.is_dir() {
        if !actual_metadata.is_dir() {
            return Err(tampered_materialization(actual, "a directory was replaced"));
        }
        let mut actual_names = BTreeSet::new();
        for entry in fs::read_dir(actual)
            .map_err(|error| tampered_materialization(actual, &format!("cannot list cached directory: {error}")))?
        {
            let entry = entry
                .map_err(|error| tampered_materialization(actual, &format!("cannot list cached directory: {error}")))?;
            actual_names.insert(entry.file_name());
        }
        for entry in fs::read_dir(expected)
            .map_err(|error| tampered_materialization(actual, &format!("cannot list staged directory: {error}")))?
        {
            let entry = entry
                .map_err(|error| tampered_materialization(actual, &format!("cannot list staged directory: {error}")))?;
            let name = entry.file_name();
            if !actual_names.remove(&name) {
                return Err(tampered_materialization(&actual.join(&name), "is missing"));
            }
            verify_materialized_tree(&entry.path(), &actual.join(name))?;
        }
        if let Some(extra) = actual_names.first() {
            return Err(tampered_materialization(&actual.join(extra), "is an extra path"));
        }
        return Ok(());
    }
    if !expected_metadata.is_file() || !actual_metadata.is_file() || expected_metadata.len() != actual_metadata.len() {
        return Err(tampered_materialization(actual, "has changed type or size"));
    }
    let mut expected_file = File::open(expected)
        .map_err(|error| tampered_materialization(actual, &format!("cannot read staged file: {error}")))?;
    let mut actual_file = File::open(actual)
        .map_err(|error| tampered_materialization(actual, &format!("cannot read cached file: {error}")))?;
    let mut expected_buffer = [0_u8; 64 * 1024];
    let mut actual_buffer = [0_u8; 64 * 1024];
    let mut remaining = expected_metadata.len();
    while remaining > 0 {
        let chunk = usize::try_from(remaining.min(expected_buffer.len() as u64)).unwrap();
        expected_file
            .read_exact(&mut expected_buffer[..chunk])
            .map_err(|error| tampered_materialization(actual, &format!("cannot read staged file: {error}")))?;
        actual_file
            .read_exact(&mut actual_buffer[..chunk])
            .map_err(|error| tampered_materialization(actual, &format!("cannot read cached file: {error}")))?;
        if expected_buffer[..chunk] != actual_buffer[..chunk] {
            return Err(tampered_materialization(actual, "contains changed bytes"));
        }
        remaining -= chunk as u64;
    }
    Ok(())
}

fn tampered_materialization(path: &Path, reason: &str) -> ProjectError {
    ProjectError::Validation(format!(
        "tampered registry materialization at {} ({reason}); clean and rebuild the generated package cache",
        path.display()
    ))
}

fn create_checked_directories(output_dir: &Path, relative: &Path) -> Result<(), ProjectError> {
    let mut directory = output_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        directory.push(segment);
        match fs::symlink_metadata(&directory) {
            Ok(_) => validate_existing_path(&directory, true, false)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&directory)
                    .map_err(|source| ProjectError::MaterializationCreateDir { path: directory.clone(), source })?;
                validate_existing_path(&directory, true, false)?;
            }
            Err(error) => {
                return Err(ProjectError::Validation(format!(
                    "registry artifact extraction path cannot be inspected: {}: {error}",
                    directory.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MAX_ENTRY_UNCOMPRESSED_BYTES, check_output_limits};

    #[test]
    fn declared_archive_total_must_not_exceed_one_gib_even_when_entries_fit() {
        let mut total = 0;
        check_output_limits(MAX_ENTRY_UNCOMPRESSED_BYTES, &mut total).unwrap();
        check_output_limits(MAX_ENTRY_UNCOMPRESSED_BYTES, &mut total).unwrap();

        let error = check_output_limits(1, &mut total).expect_err("the third entry must exceed the total budget");

        assert!(error.to_string().contains("1 GiB"), "unexpected error: {error}");
    }
}
