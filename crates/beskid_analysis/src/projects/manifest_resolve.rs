//! Workspace-aware discovery of `Project.proj` paths (matches CLI `resolve_input` rules).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use super::discovery::{
    discover_project_file, discover_project_manifest_in_dir, discover_workspace_file,
    is_project_manifest_path, is_workspace_manifest_path, project_manifest_for_member_dir,
    reject_legacy_manifest_path,
};
use super::error::ProjectError;
use super::model::WorkspaceResolutionSummary;
use super::parser::parse_workspace_manifest;

/// Resolve `Workspace.proj` / member selection to a concrete `Project.proj` path and optional workspace summary.
pub fn resolve_workspace_candidate_with_summary(
    candidate: &Path,
    input: Option<&Path>,
    workspace_member: Option<&str>,
) -> Result<(PathBuf, Option<WorkspaceResolutionSummary>)> {
    let is_workspace_manifest = is_workspace_manifest_path(candidate);

    if is_workspace_manifest {
        let (path, summary) =
            resolve_project_manifest_from_workspace(candidate, input, workspace_member)?;
        Ok((path, Some(summary)))
    } else {
        Ok((candidate.to_path_buf(), None))
    }
}

/// Resolve `Workspace.proj` / member selection to a concrete `Project.proj` path.
pub fn resolve_workspace_candidate_path(
    candidate: &Path,
    input: Option<&Path>,
    workspace_member: Option<&str>,
) -> Result<PathBuf> {
    Ok(resolve_workspace_candidate_with_summary(candidate, input, workspace_member)?.0)
}

pub fn resolve_project_manifest_from_workspace(
    workspace_manifest_path: &Path,
    input: Option<&Path>,
    workspace_member: Option<&str>,
) -> Result<(PathBuf, WorkspaceResolutionSummary)> {
    let source = fs::read_to_string(workspace_manifest_path).map_err(|io_error| {
        let message = io_error.to_string();
        anyhow!(
            "{}: failed to read workspace manifest at {}: {}",
            ProjectError::ReadManifest {
                path: workspace_manifest_path.to_path_buf(),
                source: io_error,
            }
            .code(),
            workspace_manifest_path.display(),
            message,
        )
    })?;

    let workspace_manifest =
        parse_workspace_manifest(&source).map_err(|err| anyhow!("{}: {err}", err.code()))?;

    let workspace_root = workspace_manifest_path.parent().ok_or_else(|| {
        anyhow!(
            "{}: invalid workspace manifest path {}",
            ProjectError::Validation("invalid workspace manifest path".to_string()).code(),
            workspace_manifest_path.display()
        )
    })?;

    let selected_member = if let Some(member_name) = workspace_member {
        workspace_manifest
            .members
            .iter()
            .find(|member| member.name == member_name)
    } else if let Some(input_path) = input {
        workspace_manifest
            .members
            .iter()
            .filter_map(|member| {
                let candidate_root = workspace_root.join(&member.path);
                if input_path.starts_with(&candidate_root) {
                    let depth = candidate_root.components().count();
                    Some((depth, member))
                } else {
                    None
                }
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, member)| member)
            .or_else(|| {
                workspace_manifest
                    .workspace
                    .extras
                    .get("defaultTestMember")
                    .and_then(|member_id| {
                        workspace_manifest
                            .members
                            .iter()
                            .find(|member| member.name == *member_id)
                    })
            })
            .or_else(|| workspace_manifest.members.first())
    } else {
        workspace_manifest
            .workspace
            .extras
            .get("defaultTestMember")
            .and_then(|member_id| {
                workspace_manifest
                    .members
                    .iter()
                    .find(|member| member.name == *member_id)
            })
            .or(workspace_manifest.members.first())
    }
    .ok_or_else(|| {
        anyhow!(
            "{}: workspace manifest `{}` could not resolve member (requested={})",
            ProjectError::Validation("workspace has no members".to_string()).code(),
            workspace_manifest_path.display(),
            workspace_member.unwrap_or("<auto>")
        )
    })?;

    let member_dir = workspace_root.join(&selected_member.path);
    let member_manifest = project_manifest_for_member_dir(&member_dir).map_err(|err| {
        anyhow!(
            "{}: workspace member `{}` project manifest not found under {} ({err})",
            ProjectError::ProjectFileNotFound(member_dir.join("<missing>.bproj")).code(),
            selected_member.name,
            member_dir.display(),
        )
    })?;

    let summary = WorkspaceResolutionSummary {
        workspace_manifest_path: workspace_manifest_path.to_path_buf(),
        selected_member_id: selected_member.name.clone(),
    };

    Ok((member_manifest, summary))
}

/// Discover the effective `Project.proj` for a source file or directory, applying
/// `Workspace.proj` member selection when needed. Output is always a concrete
/// `Project.proj` path when `Some`, plus workspace selection metadata when applicable.
pub fn resolve_project_manifest_for_source_path(
    path: &Path,
    workspace_member: Option<&str>,
) -> Result<Option<(PathBuf, Option<WorkspaceResolutionSummary>)>> {
    let (candidate, summary_hint) = if let Some(project_manifest) = discover_project_file(path) {
        (project_manifest, None)
    } else if let Some(workspace_manifest) = discover_workspace_file(path) {
        let (member_manifest, summary) = resolve_project_manifest_from_workspace(
            &workspace_manifest,
            Some(path),
            workspace_member,
        )?;
        (member_manifest, Some(summary))
    } else {
        return Ok(None);
    };

    let (manifest, summary_merge) =
        resolve_workspace_candidate_with_summary(&candidate, Some(path), workspace_member)?;
    let summary = summary_merge.or(summary_hint);
    Ok(Some((manifest, summary)))
}

/// Discover the effective `Project.proj` from the current working directory.
pub fn resolve_project_manifest_for_cwd(
    workspace_member: Option<&str>,
) -> Result<Option<(PathBuf, Option<WorkspaceResolutionSummary>)>> {
    let Some(cwd) = env::current_dir().ok() else {
        return Ok(None);
    };

    let (candidate, summary_hint) = if let Some(project_manifest) = discover_project_file(&cwd) {
        (project_manifest, None)
    } else if let Some(workspace_manifest) = discover_workspace_file(&cwd) {
        let (member_manifest, summary) =
            resolve_project_manifest_from_workspace(&workspace_manifest, None, workspace_member)?;
        (member_manifest, Some(summary))
    } else {
        return Ok(None);
    };

    let (manifest, summary_merge) =
        resolve_workspace_candidate_with_summary(&candidate, None, workspace_member)?;
    let summary = summary_merge.or(summary_hint);
    Ok(Some((manifest, summary)))
}

/// Same rules as CLI manifest discovery without an explicit `--project` path.
pub fn discover_project_manifest_from_input_or_cwd(
    input: Option<&PathBuf>,
    workspace_member: Option<&str>,
) -> Result<Option<(PathBuf, Option<WorkspaceResolutionSummary>)>> {
    if let Some(input) = input {
        resolve_project_manifest_for_source_path(input, workspace_member)
    } else {
        resolve_project_manifest_for_cwd(workspace_member)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::resolve_project_manifest_from_workspace;
    use crate::projects::parse_workspace_manifest;

    fn corelib_workspace_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/CoreLib.bws")
    }

    #[test]
    fn corelib_workspace_default_test_member_resolves_tests_project() {
        let path = corelib_workspace_path();
        let src = std::fs::read_to_string(&path).expect("read CoreLib.bws");
        let manifest = parse_workspace_manifest(&src).expect("parse workspace");
        assert_eq!(
            manifest
                .workspace
                .extras
                .get("defaultTestMember")
                .map(String::as_str),
            Some("corelib_tests")
        );

        let (member_manifest, summary) =
            resolve_project_manifest_from_workspace(&path, None, None).expect("resolve member");
        assert!(
            member_manifest
                .components()
                .any(|part| part.as_os_str() == "corelib_tests.bproj"),
            "expected corelib_tests.bproj, got {}",
            member_manifest.display()
        );
        assert_eq!(summary.selected_member_id, "corelib_tests");
    }
}
