//! Pure dependency intent edits. Resolution and transactional writes are separate seams.
#![allow(non_snake_case)]

use std::path::PathBuf;

use super::{DependencySource, ProjectError, parse_bsol_document, parse_manifest};

#[path = "manifest_edit.rs"]
mod manifest_edit;
mod selection;

pub use super::workflow::{CommitDependencyChange, DependencyChangePlan, DependencyChangeReport, PlanDependencyChange};
pub use selection::SelectDependencyProject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyIntent {
    pub name: String,
    pub source: DependencyIntentSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyIntentSource {
    Registry { registry: Option<String>, version: String },
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyMutation {
    Add(DependencyIntent),
    Remove(String),
    Update { package: Option<String>, version: Option<String>, all: bool },
}

/// Preserve untouched bytes; validate both original and resulting manifest contracts.
pub fn EditManifest(original: &str, mutation: &DependencyMutation) -> Result<String, ProjectError> {
    let manifest = parse_manifest(original)?;
    let document = parse_bsol_document(original).map_err(|error| ProjectError::from_bsol(error.into()))?;
    let blocks = manifest_edit::DependencyBlocks(&document)?;
    let mut edits = Vec::new();
    match mutation {
        DependencyMutation::Add(intent) => {
            manifest_edit::ValidateIntent(intent)?;
            if let Some(existing) = manifest.dependencies.iter().find(|dependency| dependency.name == intent.name) {
                let matches = match &intent.source {
                    DependencyIntentSource::Registry { registry, version } => {
                        existing.source == DependencySource::Registry
                            && existing.registry == *registry
                            && existing.version.as_deref() == Some(version.as_str())
                    }
                    DependencyIntentSource::Path(path) => {
                        existing.source == DependencySource::Path && existing.path.as_deref() == path.to_str()
                    }
                };
                if matches {
                    return Ok(original.to_owned());
                }
                return Err(ProjectError::Validation(format!(
                    "dependency `{}` already has different intent; use dep update or remove it explicitly",
                    intent.name
                )));
            }
            let newline = manifest_edit::Newline(original);
            let mut addition = String::new();
            if !original.is_empty() && !original.ends_with('\n') {
                addition.push_str(newline);
            }
            addition.push_str(&manifest_edit::RenderIntent(intent, newline)?);
            edits.push(manifest_edit::Edit { start: original.len(), end: original.len(), replacement: addition });
        }
        DependencyMutation::Remove(name) => {
            manifest_edit::Nonempty(name, "dependency name")?;
            if let Some(block) = blocks.get(name.as_str()) {
                edits.push(manifest_edit::Edit {
                    start: block.span.start,
                    end: block.span.end,
                    replacement: String::new(),
                });
            }
        }
        DependencyMutation::Update { package, version, all } => {
            if package.is_some() == *all || (*all && version.is_some()) {
                return Err(ProjectError::Validation(
                    "dep update requires one package or --all; an explicit version requires one package".into(),
                ));
            }
            if let Some(name) = package {
                manifest_edit::Nonempty(name, "dependency name")?;
                let dependency = manifest
                    .dependencies
                    .iter()
                    .find(|dependency| dependency.name == *name)
                    .ok_or_else(|| ProjectError::Validation(format!("dependency `{name}` is not declared")))?;
                if let Some(version) = version {
                    manifest_edit::Nonempty(version, "resolved dependency version")?;
                    if dependency.source != DependencySource::Registry {
                        return Err(ProjectError::Validation(format!(
                            "dependency `{name}` is not a registry dependency; an explicit version cannot change its source"
                        )));
                    }
                    let block = blocks.get(name.as_str()).ok_or_else(|| {
                        ProjectError::Validation(format!("dependency `{name}` syntax is unavailable"))
                    })?;
                    let span = manifest_edit::VersionSpan(block)?;
                    edits.push(manifest_edit::Edit {
                        start: span.start,
                        end: span.end,
                        replacement: manifest_edit::Quoted(version)?,
                    });
                }
            }
            // Versionless refresh changes only resolver-owned lock intent, not source bytes.
        }
    }
    let updated = manifest_edit::Apply(original, edits)?;
    parse_manifest(&updated)?;
    Ok(updated)
}
