//! Lower schema-validated Bsol documents into typed project / workspace models.

mod document_validation;
mod fields_errors;
mod intermediate;
mod project_lowering;
mod sections_builders;
mod workspace_lowering;

use super::{
    error::ProjectError,
    model::{ProjectManifest, WorkspaceManifest},
    validator::{validate_manifest, validate_workspace_manifest},
};
use document_validation::{parse_project_document, parse_workspace_document};
use project_lowering::lower_project_document;
use sections_builders::build_manifest;
use workspace_lowering::{build_workspace_manifest, lower_workspace_document};

pub fn parse_manifest(source: &str) -> Result<ProjectManifest, ProjectError> {
    let validated = parse_project_document(source)?;
    let parsed = lower_project_document(validated)?;
    let manifest = build_manifest(parsed)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn parse_workspace_manifest(source: &str) -> Result<WorkspaceManifest, ProjectError> {
    let validated = parse_workspace_document(source)?;
    let parsed = lower_workspace_document(validated)?;
    let manifest = build_workspace_manifest(parsed)?;
    validate_workspace_manifest(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests;
