//! Map Beskid parse/semantic/manifest errors to LSP [`Diagnostic`] values.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::{parse_bsol_document, parse_manifest, parse_workspace_manifest, ProjectError};
use beskid_analysis::services::{
    self, DocumentAnalysisSnapshot, FrontEndOptions, PrepareOptions, resolved_input_from_plan,
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
/// [`CompilationContext`] are available; otherwise the server falls back to a warm
/// [`DocumentAnalysisSnapshot`] or parse-only structural rules.
pub fn analyze_document(
    db: Option<&mut BeskidDatabase>,
    uri: &Uri,
    source: &str,
    cached: Option<&DocumentAnalysisSnapshot>,
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
            if let Ok((_, mut diags)) = beskid_queries::prepare_compilation_diagnostics_with_db(
                db,
                &resolved,
                PrepareOptions {
                    front_end: FrontEndOptions {
                        with_semantic_diagnostics: true,
                        ..Default::default()
                    },
                },
                None,
            ) {
                if let Some(snap) = cached {
                    diags.extend(snap.doc_diagnostics.iter().cloned());
                }
                return diags
                    .into_iter()
                    .map(|diag| semantic_to_lsp_diagnostic(source, diag))
                    .collect();
            }
        }

        if let Some(snap) = cached {
            return diagnostics_from_cached_snapshot(uri, source, snap);
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

fn diagnostics_from_cached_snapshot(
    uri: &Uri,
    source: &str,
    snap: &DocumentAnalysisSnapshot,
) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> =
        semantic_diagnostics(&uri.to_string(), source, &snap.program.node);
    out.extend(
        snap.doc_diagnostics
            .iter()
            .cloned()
            .map(|d| semantic_to_lsp_diagnostic(source, d)),
    );
    out.extend(
        snap.composition_diagnostics
            .iter()
            .cloned()
            .map(|d| semantic_to_lsp_diagnostic(source, d)),
    );
    out.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character)
            .cmp(&(b.range.start.line, b.range.start.character))
    });
    out
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

    use tower_lsp_server::ls_types::{NumberOrString, Uri};

    use beskid_analysis::services::{build_document_analysis, parse_program_with_source_name};

    use super::analyze_document;

    #[test]
    fn lsp_cached_snapshot_surfaces_composition_diagnostic_codes() {
        let uri = Uri::from_str("file:///composition.bd").expect("uri");
        let source = r#"
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
        let program =
            parse_program_with_source_name("composition.bd", source).expect("parse source");
        let snapshot = build_document_analysis(&program, "composition.bd", source, None);
        let diagnostics = analyze_document(None, &uri, source, Some(&snapshot), None);
        let codes = diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code.as_ref())
            .filter_map(|code| match code {
                NumberOrString::String(value) => Some(value.clone()),
                NumberOrString::Number(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            codes
                .iter()
                .filter(|code| code.starts_with("E17"))
                .collect::<Vec<_>>(),
            vec!["E1709"],
            "warm snapshot path should surface only expected composition code"
        );
    }
}
