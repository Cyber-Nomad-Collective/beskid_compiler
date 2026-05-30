//! Map Beskid parse/semantic/manifest errors to LSP [`Diagnostic`] values.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::{parse_manifest, parse_workspace_manifest};
use beskid_analysis::services::{self, DocumentAnalysisSnapshot};
use beskid_analysis::syntax::Program;
use beskid_analysis::{SemanticDiagnostic, Severity};
use tower_lsp_server::ls_types::*;

use crate::features::project_manifest::api as project_manifest;
use crate::position::offset_range_to_lsp;

/// Produce LSP diagnostics for a `.bd`, `.proj`, or manifest buffer using the prepare spine when a project is known.
pub fn analyze_document(
    uri: &Uri,
    source: &str,
    cached: Option<&DocumentAnalysisSnapshot>,
    compilation_context: Option<&CompilationContext>,
) -> Vec<Diagnostic> {
    if is_project_manifest_uri(uri) {
        return analyze_project_manifest(uri, source);
    }

    if let Some(path) = uri.to_file_path()
        && path.extension().and_then(|ext| ext.to_str()) == Some("bd")
    {
        let mut ctx = compilation_context
            .cloned()
            .or_else(|| CompilationContext::try_for_analysis_path(&path, None));
        if let Some(ref mut ctx) = ctx
            && let Ok(mut diags) =
                services::analyze_source_with_compilation_context(path.as_ref(), source, ctx)
        {
            if let Some(snap) = cached {
                diags.extend(snap.doc_diagnostics.iter().cloned());
            }
            return diags
                .into_iter()
                .map(|diag| semantic_to_lsp_diagnostic(source, diag))
                .collect();
        }

        if services::compile_plan_for_input_path(&path).is_some() {
            if let Ok(diagnostics) = services::analyze_source_in_project(path.as_ref(), source) {
                return diagnostics
                    .into_iter()
                    .map(|diag| semantic_to_lsp_diagnostic(source, diag))
                    .collect();
            }
            return vec![simple_error(
                "project",
                "project analysis failed; open the workspace entry or reload the language server",
                Range::new(Position::new(0, 0), Position::new(0, 0)),
            )];
        }

        if let Some(snap) = cached {
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
            return out;
        }
    }

    if let Some(project_diags) = analyze_project_file(uri, source) {
        return project_diags;
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

fn analyze_project_file(uri: &Uri, source: &str) -> Option<Vec<Diagnostic>> {
    let path = uri.to_file_path()?;
    let diagnostics = services::analyze_source_in_project(path.as_ref(), source).ok()?;
    Some(
        diagnostics
            .into_iter()
            .map(|diag| semantic_to_lsp_diagnostic(source, diag))
            .collect(),
    )
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
        "Workspace.proj"
    } else {
        "Project.proj"
    };
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

fn is_project_manifest_uri(uri: &Uri) -> bool {
    uri.to_string().to_lowercase().ends_with(".proj")
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

i32 main() {
    launch MissingHost();
    return 0;
}
"#;
        let program =
            parse_program_with_source_name("composition.bd", source).expect("parse source");
        let snapshot = build_document_analysis(&program, "composition.bd", source, None);
        let diagnostics = analyze_document(&uri, source, Some(&snapshot), None);
        let codes = diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code.as_ref())
            .filter_map(|code| match code {
                NumberOrString::String(value) => Some(value.clone()),
                NumberOrString::Number(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            codes.iter().filter(|code| code.starts_with("E17")).collect::<Vec<_>>(),
            vec!["E1709"],
            "warm snapshot path should surface only expected composition code"
        );
    }
}
