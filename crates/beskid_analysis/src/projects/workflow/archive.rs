use std::fs;
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};

use tempfile::NamedTempFile;
use zip::ZipArchive;

use crate::projects::error::ProjectError;

pub(super) fn extract_zip_to_dir(file: File, output_dir: &Path) -> Result<(), ProjectError> {
    let mut archive = ZipArchive::new(file)
        .map_err(|err| ProjectError::Validation(format!("invalid registry artifact ZIP: {err}")))?;

    // Inspect the whole archive before changing the destination. A later entry
    // may traverse an already materialized directory through a symlink.
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        let path = checked_entry_name(&entry)?;
        validate_destination(output_dir, &path, entry.is_dir())?;
    }

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
        io::copy(&mut entry, scratch.as_file_mut()).map_err(|source| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target.clone(),
            source,
        })?;
        validate_destination(output_dir, &path, false)?;
        scratch.persist(&target).map_err(|error| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target,
            source: error.error,
        })?;
    }

    Ok(())
}

fn checked_entry_name(entry: &zip::read::ZipFile<'_>) -> Result<PathBuf, ProjectError> {
    let path = entry
        .enclosed_name()
        .ok_or_else(|| ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()))?;
    if path.as_os_str().is_empty()
        || entry.is_symlink()
        || entry.unix_mode().is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o040000 | 0o100000))
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
