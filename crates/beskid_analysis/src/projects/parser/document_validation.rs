use bsol::{ValidatedDocument, load_profile, parse_bsol_document, validate};

use super::super::error::ProjectError;

pub(super) fn parse_project_document(source: &str) -> Result<ValidatedDocument, ProjectError> {
    let document = parse_bsol_document(source).map_err(|e| ProjectError::from_bsol(bsol::BsolError::from(e)))?;
    let profile = load_profile("project.v1").map_err(ProjectError::from_bsol)?;
    validate(&document, &profile).map_err(ProjectError::from_bsol)
}

pub(super) fn parse_workspace_document(source: &str) -> Result<ValidatedDocument, ProjectError> {
    let document = parse_bsol_document(source).map_err(|e| ProjectError::from_bsol(bsol::BsolError::from(e)))?;
    let profile = load_profile("workspace.v1").map_err(ProjectError::from_bsol)?;
    validate(&document, &profile).map_err(ProjectError::from_bsol)
}
