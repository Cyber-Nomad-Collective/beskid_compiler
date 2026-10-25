//! Deterministic project selection without implicit first-workspace-member fallback.
use crate::projects::{
    ProjectError, is_project_manifest_path, is_workspace_manifest_path, parse_manifest, parse_workspace_manifest,
    reject_legacy_manifest_path,
};
use std::fs;
use std::path::{Path, PathBuf};

pub fn select_dependency_project(start: &Path, explicit: Option<&Path>) -> Result<PathBuf, ProjectError> {
    if let Some(explicit) = explicit {
        let base = if start.is_dir() { start } else { start.parent().unwrap_or(Path::new(".")) };
        let selected = if explicit.is_absolute() { explicit.to_path_buf() } else { base.join(explicit) };
        return select_candidate(&selected, start);
    }
    if start.is_file() {
        return select_candidate(start, start);
    }
    let mut directory = start.to_path_buf();
    loop {
        let (projects, workspaces) = directory_candidates(&directory)?;
        if !projects.is_empty() {
            return unique_project(projects);
        }
        if workspaces.len() > 1 {
            return Err(ambiguous(&workspaces));
        }
        if let Some(workspace) = workspaces.first() {
            return select_workspace(workspace, start);
        }
        if !directory.pop() {
            return Err(ProjectError::ProjectFileNotFound(start.to_path_buf()));
        }
    }
}
fn read(path: &Path) -> Result<String, ProjectError> {
    fs::read_to_string(path).map_err(|source| ProjectError::ReadManifest { path: path.to_path_buf(), source })
}
fn select_candidate(candidate: &Path, start: &Path) -> Result<PathBuf, ProjectError> {
    reject_legacy_manifest_path(candidate)?;
    if candidate.is_dir() {
        let (projects, workspaces) = directory_candidates(candidate)?;
        if !projects.is_empty() {
            return unique_project(projects);
        }
        if workspaces.len() == 1 {
            return select_workspace(&workspaces[0], start);
        }
        if workspaces.len() > 1 {
            return Err(ambiguous(&workspaces));
        }
        return Err(ProjectError::ProjectFileNotFound(candidate.to_path_buf()));
    }
    if is_workspace_manifest_path(candidate) {
        return select_workspace(candidate, start);
    }
    if !is_project_manifest_path(candidate) {
        return Err(ProjectError::Validation(format!(
            "--project expects a .bproj project or .bws workspace, found `{}`",
            candidate.display()
        )));
    }
    parse_manifest(&read(candidate)?)?;
    Ok(candidate.to_path_buf())
}
fn directory_candidates(directory: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), ProjectError> {
    let mut projects = Vec::new();
    let mut workspaces = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|source| ProjectError::ReadManifest { path: directory.to_path_buf(), source })?
    {
        let entry = entry.map_err(|source| ProjectError::ReadManifest { path: directory.to_path_buf(), source })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        reject_legacy_manifest_path(&path)?;
        if is_project_manifest_path(&path) {
            projects.push(path);
        } else if is_workspace_manifest_path(&path) {
            workspaces.push(path);
        }
    }
    projects.sort();
    workspaces.sort();
    Ok((projects, workspaces))
}
fn unique_project(mut projects: Vec<PathBuf>) -> Result<PathBuf, ProjectError> {
    projects.sort();
    projects.dedup();
    match projects.as_slice() {
        [selected] => {
            parse_manifest(&read(selected)?)?;
            Ok(selected.clone())
        }
        [] => Err(ProjectError::Validation("no project available; specify --project <path.bproj>".into())),
        _ => Err(ambiguous(&projects)),
    }
}
fn ambiguous(candidates: &[PathBuf]) -> ProjectError {
    let mut sorted = candidates.to_vec();
    sorted.sort();
    sorted.dedup();
    ProjectError::Validation(format!(
        "dependency project selection is ambiguous: {}; specify --project <path.bproj>",
        sorted.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
    ))
}
fn select_workspace(path: &Path, start: &Path) -> Result<PathBuf, ProjectError> {
    let workspace = parse_workspace_manifest(&read(path)?)?;
    let root = path.parent().unwrap_or(Path::new("."));
    let mut candidates = Vec::new();
    let mut contextual = Vec::new();
    let start_identity = fs::canonicalize(start).ok();
    for member in &workspace.members {
        let member_path = root.join(&member.path);
        let member_projects = if member_path.is_file() && is_project_manifest_path(&member_path) {
            vec![member_path.clone()]
        } else {
            directory_candidates(&member_path)?.0
        };
        if member_projects.is_empty() {
            return Err(ProjectError::ProjectFileNotFound(member_path));
        }
        if let (Some(start), Ok(member_root)) = (&start_identity, fs::canonicalize(&member_path)) {
            if start.starts_with(&member_root) {
                contextual.extend(member_projects.iter().cloned());
            }
        }
        candidates.extend(member_projects);
    }
    if !contextual.is_empty() {
        return unique_project(contextual);
    }
    unique_project(candidates)
}
