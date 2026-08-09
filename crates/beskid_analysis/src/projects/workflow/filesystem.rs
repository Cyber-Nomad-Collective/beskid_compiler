use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::projects::error::ProjectError;

fn should_skip_materialized_subdir(name: Option<&str>) -> bool {
    matches!(name, Some("obj") | Some("tests"))
}

pub(super) fn copy_directory_when_newer(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(destination)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: destination.to_path_buf(), source })?;

    for entry in fs::read_dir(source)
        .map_err(|err| ProjectError::MaterializationReadDir { path: source.to_path_buf(), source: err })?
    {
        let entry =
            entry.map_err(|err| ProjectError::MaterializationReadDir { path: source.to_path_buf(), source: err })?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|source| ProjectError::MaterializationMetadata { path: entry_path.clone(), source })?;

        if file_type.is_dir() {
            if should_skip_materialized_subdir(entry.file_name().to_str()) {
                continue;
            }
            copy_directory_when_newer(&entry_path, &destination_path)?;
            continue;
        }

        if file_type.is_file() {
            copy_file_when_newer(&entry_path, &destination_path)?;
        }
    }

    Ok(())
}

fn copy_file_when_newer(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    let should_copy = if destination.is_file() {
        let source_modified = fs::metadata(source)
            .and_then(|metadata| metadata.modified())
            .map_err(|err| ProjectError::MaterializationMetadata { path: source.to_path_buf(), source: err })?;
        let destination_modified = fs::metadata(destination)
            .and_then(|metadata| metadata.modified())
            .map_err(|source| ProjectError::MaterializationMetadata { path: destination.to_path_buf(), source })?;
        if source_modified > destination_modified { true } else { !file_contents_equal(source, destination)? }
    } else {
        true
    };

    if should_copy {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| ProjectError::MaterializationCreateDir { path: parent.to_path_buf(), source })?;
        }
        fs::copy(source, destination).map_err(|err| ProjectError::MaterializationCopy {
            from: source.to_path_buf(),
            to: destination.to_path_buf(),
            source: err,
        })?;
    }

    Ok(())
}

fn file_contents_equal(source: &Path, destination: &Path) -> Result<bool, ProjectError> {
    let source_bytes = fs::read(source)
        .map_err(|err| ProjectError::MaterializationMetadata { path: source.to_path_buf(), source: err })?;
    let destination_bytes = fs::read(destination)
        .map_err(|err| ProjectError::MaterializationMetadata { path: destination.to_path_buf(), source: err })?;
    Ok(source_bytes == destination_bytes)
}

pub(super) fn materialized_dependency_id(project_name: &str, manifest_path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    manifest_path.to_string_lossy().hash(&mut hasher);
    let hash = hasher.finish();
    format!("{}-{hash:016x}", sanitize_segment(project_name))
}

pub(super) fn sanitize_segment(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() { "dependency".to_string() } else { result }
}
