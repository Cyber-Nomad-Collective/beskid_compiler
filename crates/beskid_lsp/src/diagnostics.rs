//! Map Beskid parse/semantic/manifest errors to generation-bound syntax diagnostic facts
//! and LSP [`Diagnostic`] values.
//!
//! Production publish/refresh paths consume [`SyntaxDiagnostic`] facts attached to the
//! current buffer revision. They never read `Document.analysis` or HIR snapshots.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::{
    ProjectError, parse_bsol_document, parse_manifest, parse_workspace_manifest,
};
use beskid_analysis::services::{
    self, FrontEndOptions, PrepareOptions, resolved_input_from_plan,
};
use beskid_analysis::syntax::Program;
use beskid_analysis::{SemanticDiagnostic, Severity};
use beskid_queries::BeskidDatabase;
use tower_lsp_server::ls_types::*;

use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::is_manifest_uri;
use crate::position::offset_range_to_lsp;
use crate::session::store::{SyntaxDiagnostic, SyntaxDiagnosticSeverity};

/// Collect generation-bound diagnostic facts for a `.bd`, `.bproj`, `.bws`, or other manifest buffer.
///
/// Project-backed `.bd` buffers use the Salsa prepare spine only when the typed bundle matches
/// the current file revision. A stale typed generation fails closed to parse/structural facts
/// for the current buffer — never prepare-spine diagnostics keyed to a prior generation.
pub fn collect_syntax_diagnostics(
    db: Option<&mut BeskidDatabase>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<SyntaxDiagnostic> {
    if is_manifest_uri(uri) {
        return analyze_project_manifest(uri, source);
    }

    if let Some(path) = uri.to_file_path()
        && path.extension().and_then(|ext| ext.to_str()) == Some("bd")
        && let (Some(db), Some(ctx)) = (db, compilation_context)
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
        // Fail closed: stale typed generation must not publish prepare-spine diagnostics.
        if beskid_queries::is_typed_bundle_stale(db, &entry_key) {
            return structural_syntax_diagnostics(&uri.to_string(), source);
        }
        if let Ok((_, diags)) = beskid_queries::prepare_compilation_diagnostics_with_db(
            db,
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
                dependency_typing: beskid_analysis::services::DependencyTypingPolicy::FullClosure,
            },
            None,
        ) {
            return diags
                .into_iter()
                .map(syntax_diagnostic_from_semantic)
                .collect();
        }
    }

    structural_syntax_diagnostics(&uri.to_string(), source)
}

/// Convert generation-bound facts into LSP diagnostics for the given source text.
pub fn lsp_diagnostics_from_syntax(
    source: &str,
    facts: &[SyntaxDiagnostic],
) -> Vec<Diagnostic> {
    facts
        .iter()
        .map(|fact| syntax_to_lsp_diagnostic(source, fact))
        .collect()
}

/// Produce LSP diagnostics for a buffer (one-shot callers / tests).
///
/// Prefer [`collect_syntax_diagnostics`] + [`lsp_diagnostics_from_syntax`] for publish paths
/// that already hold generation-bound Document facts.
#[cfg_attr(not(test), allow(dead_code))]
pub fn analyze_document(
    db: Option<&mut BeskidDatabase>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<Diagnostic> {
    let facts = collect_syntax_diagnostics(db, uri, source, compilation_context);
    lsp_diagnostics_from_syntax(source, &facts)
}

fn structural_syntax_diagnostics(source_name: &str, source: &str) -> Vec<SyntaxDiagnostic> {
    match services::parse_program_with_source_name(source_name, source) {
        Ok(program) => semantic_diagnostics(source_name, source, &program.node),
        Err(err) => vec![SyntaxDiagnostic {
            start: 0,
            end: 0,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("parse".to_string()),
            message: format!("{err:#}"),
        }],
    }
}

fn semantic_diagnostics(source_name: &str, source: &str, program: &Program) -> Vec<SyntaxDiagnostic> {
    services::semantic_rule_diagnostics_for_program(
        program,
        source_name.to_string(),
        source,
        AnalysisOptions::default(),
    )
    .into_iter()
    .map(syntax_diagnostic_from_semantic)
    .collect()
}

fn syntax_diagnostic_from_semantic(diag: SemanticDiagnostic) -> SyntaxDiagnostic {
    let start = diag.span.offset();
    let len = diag.span.len();
    let end = start.saturating_add(len.max(1));
    SyntaxDiagnostic {
        start,
        end,
        severity: match diag.severity {
            Severity::Error => SyntaxDiagnosticSeverity::Error,
            Severity::Warning => SyntaxDiagnosticSeverity::Warning,
            Severity::Note => SyntaxDiagnosticSeverity::Note,
        },
        code: diag.code,
        message: diag.message,
    }
}

fn syntax_to_lsp_diagnostic(source: &str, fact: &SyntaxDiagnostic) -> Diagnostic {
    Diagnostic {
        range: offset_range_to_lsp(source, fact.start, fact.end.max(fact.start.saturating_add(1))),
        severity: Some(match fact.severity {
            SyntaxDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            SyntaxDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
            SyntaxDiagnosticSeverity::Note => DiagnosticSeverity::INFORMATION,
        }),
        code: fact.code.clone().map(NumberOrString::String),
        source: Some("beskid".to_string()),
        message: fact.message.clone(),
        ..Diagnostic::default()
    }
}

fn analyze_project_manifest(uri: &Uri, source: &str) -> Vec<SyntaxDiagnostic> {
    let source_label = if project_manifest::is_workspace_manifest_uri(uri) {
        "workspace manifest"
    } else {
        "project manifest"
    };

    if let Err(err) = parse_bsol_document(source) {
        let error = ProjectError::from_bsol(err.into());
        return vec![syntax_diagnostic_from_semantic(
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
        Some(error) => vec![syntax_diagnostic_from_semantic(
            services::project_error_diagnostic(source_label, source, &error),
        )],
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use beskid_analysis::services::resolved_input_from_plan;
    use beskid_queries::{
        BeskidDatabase, bump_file_revision, configure_db_for_project, fingerprint_key,
        is_typed_bundle_stale, session_fingerprint,
    };
    use tower_lsp_server::ls_types::{NumberOrString, Uri};

    use super::{
        analyze_document, collect_syntax_diagnostics, lsp_diagnostics_from_syntax,
        structural_syntax_diagnostics,
    };
    use crate::session::lifecycle::ANALYSIS_CACHE_VERSION;
    use crate::session::store::{Document, SyntaxDiagnostic, SyntaxDiagnosticSeverity};
    use crate::workspace_scan::path_to_uri;

    #[test]
    fn diagnostics_facts_work_without_legacy_analysis_snapshot() {
        let uri = Uri::from_str("file:///no_analysis.bd").expect("uri");
        let source = "i32 Main() { return 0; }";
        let facts = collect_syntax_diagnostics(None, &uri, source, None);
        let doc = Document {
            version: 1,
            text: source.to_string(),
            analysis_cache_version: ANALYSIS_CACHE_VERSION,
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: facts,
        };

        let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic.code.as_ref() != Some(&NumberOrString::String("E1709".to_string()))
            }),
            "no-analysis structural path must not invent composition diagnostics: {diagnostics:#?}",
        );
    }

    #[test]
    fn analyze_document_uses_current_buffer_not_orphaned_analysis_snapshot() {
        let uri = Uri::from_str("file:///stale_snapshot.bd").expect("uri");
        // Historical E1709 composition error lives only in an orphaned analysis snapshot shape.
        // Publish/refresh must describe the current buffer without that snapshot.
        let diagnostics = analyze_document(None, &uri, "i32 Main() { return 0; }", None);

        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic.code.as_ref() != Some(&NumberOrString::String("E1709".to_string()))
            }),
            "diagnostics must describe the current buffer rather than a stale analysis snapshot: {diagnostics:#?}",
        );
    }

    #[test]
    fn stale_typed_generation_fails_closed_to_structural_diagnostics() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("compiler workspace root")
            .to_path_buf();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");

        let main_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let source = std::fs::read_to_string(&main_path).expect("read Main.bd");
        let project_root = main_path
            .parent()
            .and_then(|p| p.parent())
            .expect("fixture root")
            .to_path_buf();
        let uri = path_to_uri(&main_path).expect("file uri");

        let ctx = beskid_analysis::CompilationContext::try_for_analysis_path(&main_path, None)
            .expect("compilation context");
        let plan = ctx
            .compile_plan
            .clone()
            .expect("corelib_mvp fixture must expose a compile plan");
        // Match the exact ResolvedInput / entry key that collect_syntax_diagnostics builds.
        let resolved = resolved_input_from_plan(
            main_path.clone(),
            source.clone(),
            plan,
            None,
            None,
        );

        let project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.clone());
        configure_db_for_project(&project_root);
        let mut db = BeskidDatabase::with_persistence(&project_root);
        db.ensure_file_text(main_path.clone(), source.clone());

        let entry_key = session_fingerprint(&resolved)
            .map(|fp| fingerprint_key(&fp))
            .expect("entry fingerprint");
        bump_file_revision(&mut db, &entry_key);
        assert!(
            is_typed_bundle_stale(&db, &entry_key),
            "test requires a stale typed bundle after file revision bump"
        );

        let expected = structural_syntax_diagnostics(uri.as_str(), &source);
        let collected = collect_syntax_diagnostics(Some(&mut db), &uri, &source, Some(&ctx));
        std::env::set_current_dir(previous).expect("restore cwd");

        assert_eq!(
            collected, expected,
            "stale typed generation must fail closed to structural diagnostics for the current buffer"
        );
        assert!(
            collected.iter().all(|diag| diag.code.as_deref() != Some("W1504")),
            "stale generation must not emit prepare-spine diagnostics such as W1504: {collected:#?}",
        );
    }

    #[test]
    fn lsp_mapping_preserves_syntax_diagnostic_identity() {
        let source = "i32 Main() { return 0; }";
        let facts = vec![SyntaxDiagnostic {
            start: 0,
            end: 3,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("E9999".to_string()),
            message: "probe".to_string(),
        }];
        let diagnostics = lsp_diagnostics_from_syntax(source, &facts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code.as_ref(),
            Some(&NumberOrString::String("E9999".to_string()))
        );
        assert_eq!(diagnostics[0].message, "probe");
    }
}
