use std::path::{Path, PathBuf};

use crate::projects::error::ProjectError;

/// The anchor against which a portable lock path is interpreted. The caller
/// must still prove that an external project belongs to the current graph and
/// that a Corelib base is the verified installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortableLockPathBaseKind {
    LockDirectory,
    ExternalProject,
    CorelibWorkspace,
    ProjectDirectory,
    MaterializedRoot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableLockPath {
    value: String,
    base_kind: PortableLockPathBaseKind,
}

impl PortableLockPath {
    pub fn parse(field: &str, value: &str, base_kind: PortableLockPathBaseKind) -> Result<Self, ProjectError> {
        if value == "." && field == "source_root" && base_kind == PortableLockPathBaseKind::ProjectDirectory {
            return Ok(Self { value: value.to_string(), base_kind });
        }
        if value.is_empty() || value.starts_with('/') || value.contains('\\') {
            return Err(invalid_path(field));
        }
        let bytes = value.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(invalid_path(field));
        }

        let segments: Vec<&str> = value.split('/').collect();
        if segments.iter().any(|segment| segment.is_empty() || *segment == ".") {
            return Err(invalid_path(field));
        }

        let mut saw_normal_segment = false;
        for segment in &segments {
            if *segment == ".." {
                if base_kind != PortableLockPathBaseKind::ExternalProject || field != "project" || saw_normal_segment {
                    return Err(invalid_path(field));
                }
            } else {
                saw_normal_segment = true;
            }
        }
        if !saw_normal_segment && base_kind != PortableLockPathBaseKind::ExternalProject {
            return Err(invalid_path(field));
        }

        if base_kind == PortableLockPathBaseKind::MaterializedRoot
            && (segments.len() < 5 || segments[..4] != ["obj", "beskid", "deps", "src"])
        {
            return Err(ProjectError::Validation(format!("lockfile `{field}` must be beneath `obj/beskid/deps/src`")));
        }

        Ok(Self { value: value.to_string(), base_kind })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Resolve an existing source, or a possibly absent materialized output.
    /// A materialized destination is checked against symlinks in every existing
    /// ancestor before it can be used or created.
    pub fn resolve(&self, base: &Path) -> Result<PathBuf, ProjectError> {
        let base = base
            .canonicalize()
            .map_err(|_| ProjectError::Validation("lockfile path base cannot be resolved".into()))?;
        let candidate = base.join(&self.value);

        if self.base_kind != PortableLockPathBaseKind::MaterializedRoot {
            let resolved = candidate
                .canonicalize()
                .map_err(|_| ProjectError::Validation("declared lockfile path is unavailable".into()))?;
            if self.base_kind != PortableLockPathBaseKind::ExternalProject && !resolved.starts_with(&base) {
                return Err(ProjectError::Validation("lockfile path escapes its declared base".into()));
            }
            return Ok(resolved);
        }

        let owned_root = base.join("obj/beskid/deps/src");
        reject_symlinked_owned_prefix(&base)?;
        let resolved_owned_root = resolve_existing_ancestor(&owned_root)?;
        if !resolved_owned_root.starts_with(&base) {
            return Err(ProjectError::Validation("lockfile materialization root escapes the project".into()));
        }
        let resolved_candidate = resolve_existing_ancestor(&candidate)?;
        if !resolved_candidate.starts_with(&resolved_owned_root) || resolved_candidate == resolved_owned_root {
            return Err(ProjectError::Validation("lockfile materialized path escapes the dependency directory".into()));
        }
        Ok(resolved_candidate)
    }
}

fn reject_symlinked_owned_prefix(base: &Path) -> Result<(), ProjectError> {
    let mut prefix = base.to_path_buf();
    for segment in ["obj", "beskid", "deps", "src"] {
        prefix.push(segment);
        match prefix.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ProjectError::Validation("lockfile materialization root contains a symlink".into()));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ProjectError::Validation("lockfile materialization root is not a directory".into()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(ProjectError::Validation("lockfile materialization root cannot be inspected".into())),
        }
    }
    Ok(())
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, ProjectError> {
    let mut missing = Vec::new();
    let mut ancestor = path;
    loop {
        match ancestor.symlink_metadata() {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let segment = ancestor
                    .file_name()
                    .ok_or_else(|| ProjectError::Validation("lockfile path has no existing base".into()))?;
                missing.push(segment.to_os_string());
                ancestor =
                    ancestor.parent().ok_or_else(|| ProjectError::Validation("lockfile path has no parent".into()))?;
            }
            Err(_) => return Err(ProjectError::Validation("lockfile path cannot be inspected".into())),
        }
    }
    let mut resolved =
        ancestor.canonicalize().map_err(|_| ProjectError::Validation("lockfile path cannot be resolved".into()))?;
    if !missing.is_empty() && !resolved.is_dir() {
        return Err(ProjectError::Validation("lockfile path has a non-directory ancestor".into()));
    }
    for segment in missing.iter().rev() {
        resolved.push(segment);
    }
    Ok(resolved)
}

fn invalid_path(field: &str) -> ProjectError {
    ProjectError::Validation(format!("lockfile `{field}` is not a normalized portable path"))
}
