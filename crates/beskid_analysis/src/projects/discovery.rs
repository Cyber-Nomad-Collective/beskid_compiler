use std::collections::{HashSet, VecDeque};
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
    let start_dir = start_directory(start)?;
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

/// Default max depth when searching child directories for a unique manifest.
pub const DEFAULT_DESCENDANT_SEARCH_DEPTH: usize = 6;

const SKIP_DESCENDANT_DIRS: &[&str] = &[
    ".git",
    ".beskid",
    ".cargo",
    ".generated",
    "build",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

/// Search downward from `start` when ancestor lookup finds nothing (e.g. repo root above `corelib/`).
pub fn discover_workspace_file_descendant(
    start: &Path,
    max_depth: usize,
) -> Option<PathBuf> {
    let start_dir = start_directory(start)?;
    let (workspaces, _) = collect_manifests_bfs(&start_dir, max_depth);
    pick_shallowest_manifest(workspaces, &start_dir)
}

/// Search downward from `start` for a `.bproj` when ancestor lookup finds nothing.
pub fn discover_project_file_descendant(start: &Path, max_depth: usize) -> Option<PathBuf> {
    let start_dir = start_directory(start)?;
    let (_, projects) = collect_manifests_bfs(&start_dir, max_depth);
    pick_shallowest_manifest(projects, &start_dir)
}

fn start_directory(start: &Path) -> Option<PathBuf> {
    if start.is_dir() {
        Some(start.to_path_buf())
    } else {
        start.parent().map(Path::to_path_buf)
    }
}

fn collect_manifests_bfs(
    root: &Path,
    max_depth: usize,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut workspaces = Vec::new();
    let mut projects = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0usize));
    let mut seen = HashSet::new();

    while let Some((dir, depth)) = queue.pop_front() {
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if !seen.insert(canonical) {
            continue;
        }

        if let Ok(Some(manifest)) = discover_workspace_manifest_in_dir(&dir) {
            workspaces.push(manifest);
        }
        if let Ok(Some(manifest)) = discover_project_manifest_in_dir(&dir) {
            projects.push(manifest);
        }

        if depth >= max_depth {
            continue;
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skip = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SKIP_DESCENDANT_DIRS.contains(&name));
            if skip {
                continue;
            }
            queue.push_back((path, depth + 1));
        }
    }

    (workspaces, projects)
}

fn pick_shallowest_manifest(mut manifests: Vec<PathBuf>, anchor: &Path) -> Option<PathBuf> {
    if manifests.is_empty() {
        return None;
    }
    if manifests.len() == 1 {
        return manifests.pop();
    }
    manifests.sort_by_key(|path| manifest_depth_from(path, anchor));
    manifests.into_iter().next()
}

fn manifest_depth_from(path: &Path, anchor: &Path) -> usize {
    path.parent()
        .and_then(|parent| parent.strip_prefix(anchor).ok())
        .map(|relative| relative.components().count())
        .unwrap_or(usize::MAX)
}

/// Resolve the `.bproj` path for a workspace member directory.
pub fn project_manifest_for_member_dir(member_dir: &Path) -> Result<PathBuf, ProjectError> {
    discover_project_manifest_in_dir(member_dir)?.ok_or_else(|| {
        ProjectError::ProjectFileNotFound(member_dir.join("<missing>.bproj"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-discovery-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    #[test]
    fn descendant_search_finds_nested_workspace() {
        let repo = temp_root("descendant");
        let corelib = repo.join("corelib");
        fs::create_dir_all(&corelib).expect("mkdir");
        fs::write(corelib.join("CoreLib.bws"), "workspace \"CoreLib\" {}").expect("write");

        let found = discover_workspace_file_descendant(&repo, DEFAULT_DESCENDANT_SEARCH_DEPTH)
            .expect("workspace");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("CoreLib.bws"));
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn ancestor_search_finds_nearest_parent_manifest() {
        let root = temp_root("ancestor");
        fs::write(root.join("Root.bws"), "workspace \"Root\" {}").expect("write");
        let child = root.join("nested");
        fs::create_dir_all(&child).expect("mkdir");
        fs::write(child.join("Child.bws"), "workspace \"Child\" {}").expect("write");

        let found = discover_workspace_file(&child.join("src")).expect("workspace");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("Child.bws"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_prefers_ancestor_before_descendant() {
        let root = temp_root("resolve-order");
        fs::write(root.join("Root.bws"), "workspace \"Root\" {}").expect("write");
        let child = root.join("nested");
        fs::create_dir_all(&child).expect("mkdir");
        fs::write(child.join("Child.bws"), "workspace \"Child\" {}").expect("write");

        let found = discover_workspace_file(&root).expect("workspace");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("Root.bws"));
        let _ = fs::remove_dir_all(&root);
    }
}
