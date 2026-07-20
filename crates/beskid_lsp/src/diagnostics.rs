//! Map Beskid parse/semantic/manifest errors to LSP [`Diagnostic`] values.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::{
    ProjectError, parse_bsol_document, parse_manifest, parse_workspace_manifest,
};
use beskid_analysis::services::{
    self, DependencyTypingPolicy, FrontEndOptions, PrepareOptions, resolved_input_from_plan,
};
use beskid_analysis::syntax::Program;
use beskid_analysis::{SemanticDiagnostic, Severity};
use beskid_queries::BeskidDatabase;
use tower_lsp_server::ls_types::*;

use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::is_manifest_uri;
use crate::position::offset_range_to_lsp;

/// Produce LSP diagnostics for a `.bd`, `.bproj`, `.bws`, or other manifest buffer.
///
/// Project-backed `.bd` buffers use the Salsa prepare spine when a [`BeskidDatabase`] and
/// [`CompilationContext`] are available; otherwise the server uses parse-only
/// structural rules for the current source buffer.
pub fn analyze_document(
    db: Option<&mut BeskidDatabase>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<Diagnostic> {
    if is_manifest_uri(uri) {
        return analyze_project_manifest(uri, source);
    }

    if let Some(path) = uri.to_file_path()
        && path.extension().and_then(|ext| ext.to_str()) == Some("bd")
    {
        if let (Some(db), Some(ctx)) = (db, compilation_context)
            && ctx.compile_plan.is_some()
        {
            let resolved = resolved_input_from_plan(
                path.to_path_buf(),
                source.to_string(),
                ctx.compile_plan.clone().expect("compile plan"),
                None,
                None,
            );
            let entry_key = beskid_queries::session_fingerprint(&resolved)
                .map(|fp| beskid_queries::fingerprint_key(&fp))
                .unwrap_or_else(|| path.display().to_string());
            let stale = beskid_queries::is_typed_bundle_stale(db, &entry_key);
            if let Ok((_, diags)) = beskid_queries::prepare_compilation_diagnostics_with_db(
                db,
                &resolved,
                PrepareOptions {
                    front_end: FrontEndOptions {
                        with_semantic_diagnostics: true,
                        ..Default::default()
                    },
                    dependency_typing: if stale {
                        DependencyTypingPolicy::EntryOnly
                    } else {
                        DependencyTypingPolicy::FullClosure
                    },
                },
                None,
            ) {
                return diags
                    .into_iter()
                    .map(|diag| semantic_to_lsp_diagnostic(source, diag))
                    .collect();
            }
        }
    }

    match services::parse_program_with_source_name(&uri.to_string(), source) {
        Ok(program) => semantic_diagnostics(&uri.to_string(), source, &program.node),
        Err(err) => vec![simple_error(
            "parse",
            &format!("{err:#}"),
            Range::new(Position::new(0, 0), Position::new(0, 0)),
        )],
    }
}

fn semantic_diagnostics(source_name: &str, source: &str, program: &Program) -> Vec<Diagnostic> {
    services::semantic_rule_diagnostics_for_program(
        program,
        source_name.to_string(),
        source,
        AnalysisOptions::default(),
    )
    .into_iter()
    .map(|diag| semantic_to_lsp_diagnostic(source, diag))
    .collect()
}

fn semantic_to_lsp_diagnostic(source: &str, diag: SemanticDiagnostic) -> Diagnostic {
    let start = diag.span.offset();
    let len = diag.span.len();
    let end = start.saturating_add(len.max(1));
    Diagnostic {
        range: offset_range_to_lsp(source, start, end),
        severity: Some(match diag.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Note => DiagnosticSeverity::INFORMATION,
        }),
        code: diag.code.map(NumberOrString::String),
        source: Some("beskid".to_string()),
        message: diag.message,
        ..Diagnostic::default()
    }
}

fn analyze_project_manifest(uri: &Uri, source: &str) -> Vec<Diagnostic> {
    let source_label = if project_manifest::is_workspace_manifest_uri(uri) {
        "workspace manifest"
    } else {
        "project manifest"
    };

    if let Err(err) = parse_bsol_document(source) {
        let error = ProjectError::from_bsol(err.into());
        return vec![semantic_to_lsp_diagnostic(
            source,
            services::project_error_diagnostic(source_label, source, &error),
        )];
    }

    let err = if project_manifest::is_workspace_manifest_uri(uri) {
        parse_workspace_manifest(source).err()
    } else {
        parse_manifest(source).err()
    };
    match err {
        None => Vec::new(),
        Some(error) => vec![semantic_to_lsp_diagnostic(
            source,
            services::project_error_diagnostic(source_label, source, &error),
        )],
    }
}

fn simple_error(code: &str, message: &str, range: Range) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("beskid".to_string()),
        message: message.to_string(),
        ..Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use beskid_analysis::services::{build_document_analysis, parse_program_with_source_name};
    use tower_lsp_server::ls_types::{NumberOrString, Uri};

    use super::analyze_document;

    #[test]
    fn lsp_diagnostics_do_not_reuse_stale_snapshot_composition_errors() {
        let uri = Uri::from_str("file:///stale_snapshot.bd").expect("uri");
        let stale_source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Logger;
    }
}

i32 Main() {
    launch MissingHost();
    return 0;
}
"#;
        let stale_program = parse_program_with_source_name("stale_snapshot.bd", stale_source)
            .expect("parse stale source");
        let stale_snapshot =
            build_document_analysis(&stale_program, "stale_snapshot.bd", stale_source, None);
        assert!(
            stale_snapshot
                .composition_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("E1709"))
        );

        let diagnostics = analyze_document(None, &uri, "i32 Main() { return 0; }", None);

        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic.code.as_ref() != Some(&NumberOrString::String("E1709".to_string()))
            }),
            "diagnostics must describe the current buffer rather than a stale analysis snapshot: {diagnostics:#?}",
        );
    }
}
