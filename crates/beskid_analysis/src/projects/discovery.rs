use std::fs;
use std::path::{Path, PathBuf};

use crate::projects::error::ProjectError;

pub const PROJECT_MANIFEST_EXTENSION: &str = ".bproj";
pub const WORKSPACE_MANIFEST_EXTENSION: &str = ".bws";

/// Legacy names rejected at load time (hard cut).
pub const LEGACY_PROJECT_FILE_NAME: &str = "Project.proj";
pub const LEGACY_WORKSPACE_FILE_NAME: &str = "Workspace.proj";

pub fn is_project_manifest_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bproj"))
}

pub fn is_workspace_manifest_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bws"))
}

pub fn reject_legacy_manifest_path(path: &Path) -> Result<(), ProjectError> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if name == LEGACY_PROJECT_FILE_NAME {
        return Err(ProjectError::meta_contract(
            "E1894",
            "legacy `Project.proj` is no longer supported; rename to `<project.name>.bproj`",
        ));
    }
    if name == LEGACY_WORKSPACE_FILE_NAME {
        return Err(ProjectError::meta_contract(
            "E1895",
            "legacy `Workspace.proj` is no longer supported; rename to a `.bws` workspace manifest (for example `CoreLib.bws`)",
        ));
    }
    Ok(())
}

/// Exactly one `*.bproj` in `dir`, or an error describing ambiguity/absence.
pub fn discover_project_manifest_in_dir(dir: &Path) -> Result<Option<PathBuf>, ProjectError> {
    let legacy = dir.join(LEGACY_PROJECT_FILE_NAME);
    if legacy.is_file() {
        reject_legacy_manifest_path(&legacy)?;
    }
    let legacy_ws = dir.join(LEGACY_WORKSPACE_FILE_NAME);
    if legacy_ws.is_file() {
        reject_legacy_manifest_path(&legacy_ws)?;
    }

    let mut matches = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| ProjectError::ReadManifest {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ProjectError::ReadManifest {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_file() && is_project_manifest_path(&path) {
            matches.push(path);
        }
    }
    matches.sort();
    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.clone())),
        many => Err(ProjectError::Validation(format!(
            "directory `{}` contains multiple `.bproj` manifests ({})",
            dir.display(),
            many.iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Exactly one `*.bws` in `dir`, or an error describing ambiguity/absence.
pub fn discover_workspace_manifest_in_dir(dir: &Path) -> Result<Option<PathBuf>, ProjectError> {
    let legacy = dir.join(LEGACY_WORKSPACE_FILE_NAME);
    if legacy.is_file() {
        reject_legacy_manifest_path(&legacy)?;
    }

    let mut matches = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| ProjectError::ReadManifest {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ProjectError::ReadManifest {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_file() && is_workspace_manifest_path(&path) {
            matches.push(path);
        }
    }
    matches.sort();
    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.clone())),
        many => Err(ProjectError::Validation(format!(
            "directory `{}` contains multiple `.bws` workspace manifests ({})",
            dir.display(),
            many.iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

pub fn discover_project_file(start: &Path) -> Option<PathBuf> {
    let start_dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };

    let mut current = start_dir;
    loop {
        if let Ok(Some(candidate)) = discover_project_manifest_in_dir(&current) {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn discover_workspace_file(start: &Path) -> Option<PathBuf> {
    let start_dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };

    let mut current = start_dir;
    loop {
        if let Ok(Some(candidate)) = discover_workspace_manifest_in_dir(&current) {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Resolve the `.bproj` path for a workspace member directory.
pub fn project_manifest_for_member_dir(member_dir: &Path) -> Result<PathBuf, ProjectError> {
    discover_project_manifest_in_dir(member_dir)?.ok_or_else(|| {
        ProjectError::ProjectFileNotFound(member_dir.join("<missing>.bproj"))
    })
}
