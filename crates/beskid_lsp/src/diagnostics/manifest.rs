//! Diagnostics for project and workspace manifests and standalone BSOL documents.

use beskid_analysis::projects::{ProjectError, parse_bsol_document, parse_manifest, parse_workspace_manifest};
use beskid_analysis::services;
use tower_lsp_server::ls_types::Uri;

use super::to_lsp::syntax_diagnostic_from_semantic;
use crate::features::project_manifest::api as project_manifest;
use crate::session::store::SyntaxDiagnostic;

pub(super) fn analyze_project_manifest(uri: &Uri, source: &str) -> Vec<SyntaxDiagnostic> {
    let mut facts = project_manifest_facts(uri, source);
    // Manifest diagnostics are named by kind, not path; related places and markers use the buffer.
    for fact in &mut facts {
        fact.detail.uri = Some(uri.to_string());
    }
    facts
}

fn project_manifest_facts(uri: &Uri, source: &str) -> Vec<SyntaxDiagnostic> {
    let source_label =
        if project_manifest::is_workspace_manifest_uri(uri) { "workspace manifest" } else { "project manifest" };

    if let Err(err) = parse_bsol_document(source) {
        let error = ProjectError::from_bsol(err.into());
        return vec![syntax_diagnostic_from_semantic(services::project_error_diagnostic(source_label, source, &error))];
    }

    let err = if project_manifest::is_workspace_manifest_uri(uri) {
        parse_workspace_manifest(source).err()
    } else {
        parse_manifest(source).err()
    };
    match err {
        None => Vec::new(),
        Some(error) => {
            vec![syntax_diagnostic_from_semantic(services::project_error_diagnostic(source_label, source, &error))]
        }
    }
}

pub(super) fn analyze_standalone_bsol(source: &str) -> Vec<SyntaxDiagnostic> {
    match parse_bsol_document(source) {
        Ok(_) => Vec::new(),
        Err(err) => {
            let error = ProjectError::from_bsol(err.into());
            vec![syntax_diagnostic_from_semantic(services::project_error_diagnostic("BSOL document", source, &error))]
        }
    }
}
