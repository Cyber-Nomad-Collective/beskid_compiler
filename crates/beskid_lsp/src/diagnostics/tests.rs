//! Editor diagnostic tests: preparation, conversion, quick fixes and manifests.

use std::path::PathBuf;
use std::str::FromStr;

use beskid_analysis::services::resolved_input_from_plan;
use beskid_queries::{
    BeskidDatabase, bump_file_revision, configure_db_for_project, fingerprint_key, is_typed_bundle_stale,
    session_fingerprint,
};
use tower_lsp_server::ls_types::{NumberOrString, Uri};

use super::preparation::structural_syntax_diagnostics;
use super::{analyze_document, collect_syntax_diagnostics, lsp_diagnostics_from_syntax};
use crate::session::store::{Document, SyntaxDiagnostic, SyntaxDiagnosticSeverity};
use crate::workspace_scan::path_to_uri;

#[test]
fn diagnostics_facts_work_without_legacy_analysis_snapshot() {
    let uri = Uri::from_str("file:///no_analysis.bd").expect("uri");
    let source = "i32 Main() { return 0; }";
    let (facts, fixes) = collect_syntax_diagnostics(None, &uri, source, None);
    assert!(fixes.is_empty(), "structural path must not produce quick-fixes");
    let doc = Document {
        version: 1,
        text: source.to_string(),
        syntax_definitions: Vec::new(),
        syntax_hovers: Vec::new(),
        syntax_symbols: Vec::new(),
        bsol_semantic_token_candidates: Vec::new(),
        syntax_attribute_names: Vec::new(),
        syntax_completion: None,
        syntax_inlay_hints: Vec::new(),
        syntax_documentation: Vec::new(),
        syntax_diagnostics: facts,
        syntax_fixes: Vec::new(),
    };

    let diagnostics = lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code.as_ref() != Some(&NumberOrString::String("E1709".to_string())) }),
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
        diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code.as_ref() != Some(&NumberOrString::String("E1709".to_string())) }),
        "diagnostics must describe the current buffer rather than a stale analysis snapshot: {diagnostics:#?}",
    );
}

#[test]
fn standalone_bsol_document_uses_the_bsol_parser_for_diagnostics() {
    let uri = crate::workspace_scan::path_to_uri(&std::env::temp_dir().join("schema.bsol")).expect("uri");
    let source = "schema \"config\" {\n  enabled = true\n}\n";

    let (diagnostics, fixes) = collect_syntax_diagnostics(None, &uri, source, None);

    assert!(diagnostics.is_empty(), "valid standalone BSOL must not use Beskid source diagnostics: {diagnostics:#?}");
    assert!(fixes.is_empty(), "standalone BSOL does not synthesize Beskid source fixes");
}

#[test]
fn stale_typed_generation_fails_closed_to_structural_diagnostics() {
    let _fixture = crate::SHARED_FIXTURE_LOCK.blocking_lock();
    beskid_tests_mod_host::configure_shared_corelib().expect("shared Corelib");

    let main_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
    let source = std::fs::read_to_string(&main_path).expect("read Main.bd");
    let project_root = main_path.parent().and_then(|p| p.parent()).expect("fixture root").to_path_buf();
    let uri = path_to_uri(&main_path).expect("file uri");

    let ctx =
        beskid_analysis::CompilationContext::try_for_analysis_path(&main_path, None).expect("compilation context");
    let plan = ctx.compile_plan.clone().expect("corelib_mvp fixture must expose a compile plan");
    // Match the exact ResolvedInput / entry key that collect_syntax_diagnostics builds.
    let resolved = resolved_input_from_plan(main_path.clone(), source.clone(), plan, None, None);

    let project_root = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
    configure_db_for_project(&project_root);
    let mut db = BeskidDatabase::with_persistence(&project_root);
    db.ensure_file_text(main_path.clone(), source.clone());

    let entry_key = session_fingerprint(&resolved).map(|fp| fingerprint_key(&fp)).expect("entry fingerprint");
    bump_file_revision(&mut db, &entry_key);
    assert!(is_typed_bundle_stale(&db, &entry_key), "test requires a stale typed bundle after file revision bump");

    let expected = structural_syntax_diagnostics(uri.as_str(), &source);
    let mod_host = beskid_tools::native_mods::ModHostSelection::installed();
    let (collected, _fixes) = collect_syntax_diagnostics(
        Some(super::ProjectPreparation { db: &mut db, mod_host: &mod_host }),
        &uri,
        &source,
        Some(&ctx),
    );

    assert_eq!(
        collected, expected,
        "stale typed generation must fail closed to structural diagnostics for the current buffer"
    );
    assert!(
        collected.iter().all(|diag| diag.code.as_deref() != Some("W2101")),
        "stale generation must not emit prepare-spine diagnostics such as W2101: {collected:#?}",
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
        source: "beskid".to_string(),
        detail: Default::default(),
    }];
    let diagnostics = lsp_diagnostics_from_syntax(source, &facts);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code.as_ref(), Some(&NumberOrString::String("E9999".to_string())));
    assert_eq!(diagnostics[0].message, "probe");
    assert_eq!(diagnostics[0].source.as_deref(), Some("beskid"));
}

/// Assemble `Main.bd` (calls into `Lib/Helper.bd`) and `Lib/Helper.bd` (whose reachable body
/// names an unimported type) with the unit at `entry` as the open buffer.
fn dependency_assembly_input(
    entry: usize,
    generation: u64,
) -> (tempfile::TempDir, beskid_analysis::services::ResolvedInput) {
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::{parse_program_with_source_name, synthetic_compile_plan_for_source};
    use beskid_analysis::syntax::SyntaxGenerationId;
    use beskid_analysis::syntax_query::SyntaxIndex;
    use std::sync::Arc;
    let directory = tempfile::tempdir().expect("project");
    let generation = SyntaxGenerationId(generation);
    let sources = [
        ("Main.bd", "use Lib.Helper;\ni64 Main() { return Helper.Run(); }"),
        (
            "Lib/Helper.bd",
            "pub enum Slot<T> { Filled(T value), Empty() }\npub i64 Run() { Slot<Missing> slot = Slot::Empty(); return 0_i64; }",
        ),
    ];
    let units = sources
        .iter()
        .map(|(relative, source)| {
            let path = directory.path().join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(&path, source).expect("source");
            SourceUnit {
                logical_name: (*relative).into(),
                origin_path: path.clone(),
                program: parse_program_with_source_name(path.to_str().expect("path"), source).expect("parse"),
                path,
                source: (*source).into(),
            }
        })
        .collect::<Vec<_>>();
    let plan = synthetic_compile_plan_for_source(&units[entry].path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: directory.path().into() },
        dependencies: vec![],
    };
    let indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
    let index = Arc::new(ModuleIndex::build(&units, &indexes, &roots, &plan));
    let resolved = beskid_analysis::services::ResolvedInput {
        source_path: units[entry].path.clone(),
        source: sources[entry].1.into(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: Some(
            ProgramAssembly::new(
                roots,
                Arc::new(units),
                entry,
                AssemblyDiscovery::ImportClosure,
                index,
                false,
                generation,
            )
            // The LSP prepares every assembled buffer under the editor recovery policy.
            .with_recovery_policy(beskid_analysis::projects::AssemblyRecoveryPolicy::EditorRetainRecovered),
        ),
    };
    (directory, resolved)
}

/// The prepare spine reports a dependency unit's legality finding against that
/// unit's source. The LSP publishes it for the dependency's own URI (when that unit is the
/// open buffer) and never at the entry buffer's offsets.
#[test]
fn dependency_legality_finding_is_published_only_for_its_own_unit() {
    let (_main_dir, main_entry) = dependency_assembly_input(0, 511);
    let main = super::prepare_project_diagnostics_from_assembled(
        &main_entry,
        super::EditorPreparePass::Complete,
        &beskid_tools::native_mods::ModHostSelection::installed(),
    )
    .expect("main diagnostics");
    assert!(
        main.diagnostics.iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1201")),
        "the entry buffer must not carry the dependency's E1201: {:#?}",
        main.diagnostics
    );

    let (_helper_dir, helper_entry) = dependency_assembly_input(1, 512);
    let helper = super::prepare_project_diagnostics_from_assembled(
        &helper_entry,
        super::EditorPreparePass::Complete,
        &beskid_tools::native_mods::ModHostSelection::installed(),
    )
    .expect("helper diagnostics");
    let found = helper.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1201")).count();
    assert_eq!(found, 1, "the dependency's own publish carries its E1201 once: {:#?}", helper.diagnostics);
}

fn span(start: usize, end: usize) -> beskid_analysis::syntax::SpanInfo {
    beskid_analysis::syntax::SpanInfo { start, end, line_col_start: (1, 1), line_col_end: (1, 1) }
}

const IMMUTABLE: &str = "i32 Main() {\n    i32 n = 1;\n    n = 2;\n    return n;\n}\n";

fn immutable_fact() -> SyntaxDiagnostic {
    use beskid_analysis::{Applicability, Fix, FixEdit};
    let declaration = IMMUTABLE.find("n = 1").unwrap();
    let assignment = IMMUTABLE.find("n = 2").unwrap();
    let diagnostic = beskid_analysis::analysis::diagnostics::make_diagnostic(
        "file:///tmp/Main.bd",
        IMMUTABLE,
        span(declaration, declaration + 1),
        "`n` is assigned again but is not mutable.",
        "declared here",
        Some("Declare `n` with `mut`.".into()),
        Some("E1214".into()),
        beskid_analysis::Severity::Error,
    )
    .with_related(span(assignment, assignment + 1), "assigned again")
    .with_related_in("/tmp/Other.bd", "i32 X;\n", span(4, 5), "first declared here")
    .with_fix(Fix::new(
        "Make `n` mutable",
        Applicability::MachineApplicable,
        vec![FixEdit::insert(declaration, "mut ")],
    ))
    .with_fix(Fix::replace(
        "Rename the second `n`",
        span(assignment, assignment + 1),
        "m",
        Applicability::MaybeIncorrect,
    ))
    .with_tag(beskid_analysis::DiagnosticTag::Deprecated);
    super::to_lsp::syntax_diagnostic_from_semantic(diagnostic)
}

#[test]
fn editor_diagnostic_has_exact_span_code_link_help_line_related_tags_and_fix_data() {
    use tower_lsp_server::ls_types::{DiagnosticSeverity, DiagnosticTag, Position, Range};
    let diagnostics = lsp_diagnostics_from_syntax(IMMUTABLE, &[immutable_fact()]);
    assert_eq!(diagnostics.len(), 2, "the cause plus one Hint marker for the in-buffer related place");
    let cause = &diagnostics[0];
    assert_eq!(cause.range, Range { start: Position::new(1, 8), end: Position::new(1, 9) });
    assert_eq!(cause.severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(cause.code, Some(NumberOrString::String("E1214".into())));
    assert_eq!(
        cause.code_description.as_ref().map(|description| description.href.as_str()),
        Some(beskid_analysis::analysis::diagnostic_codes::REGISTRY_URL)
    );
    assert_eq!(cause.source.as_deref(), Some("beskid"));
    assert_eq!(cause.message, "`n` is assigned again but is not mutable.\nHelp: Declare `n` with `mut`.");
    assert!(!cause.message.contains("-->") && !cause.message.contains("/tmp/"));
    let related = cause.related_information.as_ref().expect("related information");
    assert_eq!(related.len(), 2);
    assert_eq!(related[0].message, "assigned again");
    assert_eq!(related[0].location.uri.as_str(), "file:///tmp/Main.bd");
    assert_eq!(related[0].location.range.start, Position::new(2, 4));
    assert_eq!(related[1].message, "first declared here");
    assert_eq!(related[1].location.uri.as_str(), "file:///tmp/Other.bd");
    assert_eq!(cause.tags.as_deref(), Some(&[DiagnosticTag::DEPRECATED][..]));
    let fixes = cause.data.as_ref().expect("fix data")["fixes"].as_array().expect("fixes").clone();
    assert_eq!(fixes.len(), 2);
    assert_eq!(fixes[0]["applicability"], "MachineApplicable");

    let marker = &diagnostics[1];
    assert_eq!(marker.severity, Some(DiagnosticSeverity::HINT));
    assert_eq!(marker.code, cause.code);
    assert_eq!(marker.message, "assigned again");
    assert_eq!(marker.range.start, Position::new(2, 4));
    let back = marker.related_information.as_ref().expect("link back to the cause");
    assert_eq!(back[0].location.range, cause.range);
    assert!(marker.data.is_none() && marker.tags.is_none());
}

#[test]
fn quick_fixes_come_from_diagnostic_data_and_only_machine_applicable_ones_are_preferred() {
    let uri = Uri::from_str("file:///tmp/Main.bd").expect("uri");
    let diagnostics = lsp_diagnostics_from_syntax(IMMUTABLE, &[immutable_fact()]);
    let actions = super::quick_fixes_from_data(&uri, IMMUTABLE, &diagnostics[0]);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].title, "Make `n` mutable");
    assert_eq!(actions[0].is_preferred, Some(true));
    let edits = &actions[0].edit.as_ref().expect("edit").changes.as_ref().expect("changes")[&uri];
    assert_eq!(edits[0].new_text, "mut ");
    assert_eq!(edits[0].range.start, edits[0].range.end, "an insertion");
    assert_eq!(actions[1].title, "Rename the second `n` (check the result)");
    assert_eq!(actions[1].is_preferred, Some(false));
    assert!(super::quick_fixes_from_data(&uri, IMMUTABLE, &diagnostics[1]).is_empty(), "markers carry no fixes");
    let edited = format!("{IMMUTABLE}// edited\n");
    assert!(
        super::quick_fixes_from_data(&uri, &edited, &diagnostics[0]).is_empty(),
        "fixes for an older revision of the buffer are refused"
    );
}

#[test]
fn unused_code_is_tagged_unnecessary() {
    use tower_lsp_server::ls_types::DiagnosticTag;
    let diagnostic = beskid_analysis::analysis::diagnostics::make_diagnostic(
        "file:///tmp/Main.bd",
        "use A.B;\n",
        span(0, 8),
        "Unused import `A.B`.",
        "never used",
        None,
        Some("W1503".into()),
        beskid_analysis::Severity::Warning,
    );
    let diagnostics =
        lsp_diagnostics_from_syntax("use A.B;\n", &[super::to_lsp::syntax_diagnostic_from_semantic(diagnostic)]);
    assert_eq!(diagnostics[0].tags.as_deref(), Some(&[DiagnosticTag::UNNECESSARY][..]));
    assert_eq!(diagnostics[0].message, "Unused import `A.B`.", "no Help line without help");
}

#[test]
fn unparsable_buffer_is_reported_at_the_parse_error_not_at_the_start() {
    let source = "i32 Main() { return 0; }\ni32 Broken( {\n";
    let facts = structural_syntax_diagnostics("file:///tmp/Broken.bd", source);
    assert!(!facts.is_empty());
    assert!(facts.iter().all(|fact| fact.start > 0), "{facts:#?}");
}

#[test]
fn preparation_failure_does_not_repeat_a_cause_the_buffer_already_reports() {
    let uri = Uri::from_str("file:///tmp/Main.bd").expect("uri");
    let structured = beskid_analysis::analysis::diagnostics::make_diagnostic(
        "file:///tmp/Main.bd",
        IMMUTABLE,
        span(5, 9),
        "Unknown type `Missing`.",
        "not found",
        None,
        Some("E1201".into()),
        beskid_analysis::Severity::Error,
    );
    let error =
        anyhow::Error::new(beskid_analysis::MietteReportError::new(structured.clone())).context("project preparation");
    let reported = vec![super::to_lsp::syntax_diagnostic_from_semantic(structured)];
    assert!(super::project_preparation_failure(&uri, &error, &reported).is_empty());
    let facts = super::project_preparation_failure(&uri, &error, &[]);
    assert_eq!(facts.len(), 1);
    assert_eq!((facts[0].start, facts[0].end), (5, 9), "reported at its exact span");
}

#[test]
fn unstructured_preparation_failure_is_one_line_and_yields_to_file_errors() {
    let uri = Uri::from_str("file:///tmp/Main.bd").expect("uri");
    let error = anyhow::anyhow!("cannot read /tmp/secret/path\n  --> detail").context("Mod artifact is stale");
    let facts = super::project_preparation_failure(&uri, &error, &[]);
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].message, "project preparation failed: Mod artifact is stale");
    assert_eq!(facts[0].code.as_deref(), Some("E3040"));
    let file_error = SyntaxDiagnostic { start: 1, end: 2, ..facts[0].clone() };
    assert!(super::project_preparation_failure(&uri, &error, &[file_error]).is_empty());
}
