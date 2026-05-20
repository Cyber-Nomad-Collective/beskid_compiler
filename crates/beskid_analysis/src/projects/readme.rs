//! Readme path resolution for `Project.proj` / `Workspace.proj` and package publishing.

use std::fs;
use std::path::{Path, PathBuf};

use crate::projects::discovery::{PROJECT_FILE_NAME, WORKSPACE_FILE_NAME};
use crate::projects::error::ProjectError;
use crate::projects::model::{ProjectManifest, WorkspaceManifest};
use crate::projects::parser::{parse_manifest, parse_workspace_manifest};

/// Default on-disk readme file name when `readme` is omitted from the manifest.
pub const DEFAULT_README_FILE: &str = "readme.md";

/// Canonical readme entry at the root of a `.bpk` artifact for pckg documentation indexing.
pub const PACKAGE_README_ARTIFACT_NAME: &str = "README.md";

/// Resolve a readme path from an explicit manifest value and on-disk defaults.
pub fn resolve_readme_relative_path(explicit: Option<&str>, package_root: &Path) -> Option<String> {
    if let Some(path) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(path.to_string());
    }

    if package_root.join(DEFAULT_README_FILE).is_file() {
        return Some(DEFAULT_README_FILE.to_string());
    }

    None
}

pub fn resolve_readme_from_project_manifest(
    package_root: &Path,
    _manifest: &ProjectManifest,
) -> Option<String> {
    resolve_readme_relative_path(None, package_root)
}

pub fn resolve_readme_from_workspace_manifest(
    package_root: &Path,
    _manifest: &WorkspaceManifest,
) -> Option<String> {
    resolve_readme_relative_path(None, package_root)
}

/// Discover readme settings for a package directory (project manifest preferred over workspace).
pub fn discover_readme_for_package_root(
    package_root: &Path,
) -> Result<Option<String>, ProjectError> {
    let project_manifest_path = package_root.join(PROJECT_FILE_NAME);
    if project_manifest_path.is_file() {
        let source = fs::read_to_string(&project_manifest_path).map_err(|source| {
            ProjectError::ReadManifest {
                path: project_manifest_path.clone(),
                source,
            }
        })?;
        let manifest = parse_manifest(&source)?;
        return Ok(resolve_readme_from_project_manifest(
            package_root,
            &manifest,
        ));
    }

    let workspace_manifest_path = package_root.join(WORKSPACE_FILE_NAME);
    if workspace_manifest_path.is_file() {
        let source = fs::read_to_string(&workspace_manifest_path).map_err(|source| {
            ProjectError::ReadManifest {
                path: workspace_manifest_path.clone(),
                source,
            }
        })?;
        let manifest = parse_workspace_manifest(&source)?;
        return Ok(resolve_readme_from_workspace_manifest(
            package_root,
            &manifest,
        ));
    }

    Ok(resolve_readme_relative_path(None, package_root))
}

/// Absolute path to the resolved readme file when present.
pub fn resolve_readme_file_path(package_root: &Path, relative: &str) -> PathBuf {
    package_root.join(relative)
}

/// Whether the zip entry already serves as the package readme for pckg (`README.md` / `readme.md` at root).
pub fn is_package_root_readme_entry(normalized_rel_path: &str) -> bool {
    if normalized_rel_path.contains('/') {
        return false;
    }
    normalized_rel_path.eq_ignore_ascii_case(PACKAGE_README_ARTIFACT_NAME)
        || normalized_rel_path.eq_ignore_ascii_case(DEFAULT_README_FILE)
}
