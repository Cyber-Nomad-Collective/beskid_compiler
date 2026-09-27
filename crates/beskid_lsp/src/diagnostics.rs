//! Map Beskid parse/semantic/manifest errors to generation-bound syntax diagnostic facts
//! and LSP [`Diagnostic`] values.
//!
//! Production publish/refresh paths consume [`SyntaxDiagnostic`] facts attached to the
//! current buffer revision. They never read `Document.analysis` or HIR snapshots.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::projects::{ProjectError, parse_bsol_document, parse_manifest, parse_workspace_manifest};
use beskid_analysis::services::{
    self, DependencyTypingPolicy, FrontEndOptions, PrepareOptions, ResolvedInput, resolved_input_from_plan,
};
use beskid_analysis::syntax::Program;
use beskid_analysis::{SemanticDiagnostic, Severity, SyntaxFix};
use beskid_queries::BeskidDatabase;
use tower_lsp_server::ls_types::*;

use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::{is_manifest_uri, is_standalone_bsol_uri};
use crate::position::offset_range_to_lsp;
use crate::session::store::{SyntaxDiagnostic, SyntaxDiagnosticSeverity};

pub(crate) struct PreparedSyntaxFacts {
    pub(crate) assembly: ProgramAssembly,
    pub(crate) diagnostics: Vec<SyntaxDiagnostic>,
    pub(crate) fixes: Vec<SyntaxFix>,
}

/// Run the one authoritative project-backed diagnostic preparation and translate its
/// generation-bound products for LSP document facts.
pub(crate) fn prepare_project_syntax_facts(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    dependency_typing: DependencyTypingPolicy,
) -> anyhow::Result<PreparedSyntaxFacts> {
    let (prepared, diagnostics, fixes) = beskid_queries::prepare_compilation_diagnostics_with_db(
        db,
        resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                with_semantic_diagnostics: dependency_typing == DependencyTypingPolicy::FullClosure,
                ..Default::default()
            },
            dependency_typing,
        },
        None,
    )?;
    let assembly = prepared.syntax_assembly();
    Ok(PreparedSyntaxFacts {
        diagnostics: entry_buffer_diagnostics(&assembly, diagnostics).map(syntax_diagnostic_from_semantic).collect(),
        assembly,
        fixes,
    })
}

/// Run downstream diagnostics from an already assembled, owned input.
///
/// The owned job uses an isolated generation-bound query database, so the LSP can
/// release its session writer before performing full dependency analysis.
pub(crate) fn prepare_project_diagnostics_from_assembled(
    resolved: &ResolvedInput,
    dependency_typing: DependencyTypingPolicy,
) -> anyhow::Result<PreparedSyntaxFacts> {
    let (prepared, diagnostics, fixes) = beskid_queries::prepare_compilation_diagnostics_isolated(
        resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                with_semantic_diagnostics: dependency_typing == DependencyTypingPolicy::FullClosure,
                ..Default::default()
            },
            dependency_typing,
        },
        None,
    )?;
    let assembly = prepared.syntax_assembly();
    Ok(PreparedSyntaxFacts {
        diagnostics: entry_buffer_diagnostics(&assembly, diagnostics).map(syntax_diagnostic_from_semantic).collect(),
        assembly,
        fixes,
    })
}

/// Keep only the diagnostics located in the entry buffer. The prepare spine also reports a
/// dependency-unit semantic-fact finding against that unit's own source; LSP facts are offsets
/// into the one buffer being published, so such a finding belongs to that unit's own publish
/// (where it is the entry) and never to this buffer's offsets.
fn entry_buffer_diagnostics(
    assembly: &ProgramAssembly,
    diagnostics: Vec<SemanticDiagnostic>,
) -> impl Iterator<Item = SemanticDiagnostic> {
    let foreign = assembly
        .units
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != assembly.entry_index)
        .map(|(_, unit)| unit.logical_name.clone())
        .collect::<std::collections::HashSet<_>>();
    diagnostics.into_iter().filter(move |diagnostic| !foreign.contains(diagnostic.src.name()))
}

/// Collect generation-bound diagnostic facts and mod-origin quick-fixes for a `.bd`,
/// `.bproj`, `.bws`, `.bsol`, or other manifest buffer.
///
/// Project-backed `.bd` buffers use the Salsa prepare spine only when the typed bundle matches
/// the current file revision. A stale typed generation fails closed to parse/structural facts
/// for the current buffer — never prepare-spine diagnostics keyed to a prior generation.
///
/// Returns `(diagnostics, fixes)` so the publish path can store both on the `Document` at the
/// same generation (re-running the spine to collect fixes separately would double the work and
/// risk a generation mismatch). Fixes are populated only on the prepare-spine path; the
/// structural and manifest paths return an empty fix list.
pub fn collect_syntax_diagnostics(
    db: Option<&mut BeskidDatabase>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> (Vec<SyntaxDiagnostic>, Vec<SyntaxFix>) {
    if is_manifest_uri(uri) {
        return (analyze_project_manifest(uri, source), Vec::new());
    }
    if is_standalone_bsol_uri(uri) {
        return (analyze_standalone_bsol(source), Vec::new());
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
            return (structural_syntax_diagnostics(&uri.to_string(), source), Vec::new());
        }
        if let Ok(prepared) = prepare_project_syntax_facts(db, &resolved, DependencyTypingPolicy::FullClosure) {
            return (prepared.diagnostics, prepared.fixes);
        }
    }

    (structural_syntax_diagnostics(&uri.to_string(), source), Vec::new())
}

/// Convert generation-bound facts into LSP diagnostics for the given source text.
pub fn lsp_diagnostics_from_syntax(source: &str, facts: &[SyntaxDiagnostic]) -> Vec<Diagnostic> {
    facts.iter().map(|fact| syntax_to_lsp_diagnostic(source, fact)).collect()
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
    let (facts, _fixes) = collect_syntax_diagnostics(db, uri, source, compilation_context);
    lsp_diagnostics_from_syntax(source, &facts)
}

fn structural_syntax_diagnostics(source_name: &str, source: &str) -> Vec<SyntaxDiagnostic> {
    match services::parse_program_with_source_name_and_diagnostics(source_name, source) {
        Ok(parsed) => {
            let mut diagnostics =
                parsed.diagnostics.into_iter().map(syntax_diagnostic_from_semantic).collect::<Vec<_>>();
            diagnostics.extend(semantic_diagnostics(source_name, source, &parsed.program.node));
            diagnostics
        }
        Err(err) => vec![SyntaxDiagnostic {
            start: 0,
            end: 0,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("parse".to_string()),
            message: format!("{err:#}"),
            source: "beskid".to_string(),
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
    // Origin tag routes code actions: `None` (compiler) → "beskid";
    // `Some("beskid:mod:<type_id>")` (mod) → that tag.
    let source = diag.origin.unwrap_or_else(|| "beskid".to_string());
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
        source,
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
        source: Some(fact.source.clone()),
        message: fact.message.clone(),
        ..Diagnostic::default()
    }
}

fn analyze_project_manifest(uri: &Uri, source: &str) -> Vec<SyntaxDiagnostic> {
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

fn analyze_standalone_bsol(source: &str) -> Vec<SyntaxDiagnostic> {
    match parse_bsol_document(source) {
        Ok(_) => Vec::new(),
        Err(err) => {
            let error = ProjectError::from_bsol(err.into());
            vec![syntax_diagnostic_from_semantic(services::project_error_diagnostic("BSOL document", source, &error))]
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use beskid_analysis::services::resolved_input_from_plan;
    use beskid_queries::{
        BeskidDatabase, bump_file_revision, configure_db_for_project, fingerprint_key, is_typed_bundle_stale,
        session_fingerprint,
    };
    use tower_lsp_server::ls_types::{NumberOrString, Uri};

    use super::{
        analyze_document, collect_syntax_diagnostics, lsp_diagnostics_from_syntax, structural_syntax_diagnostics,
    };
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

        assert!(
            diagnostics.is_empty(),
            "valid standalone BSOL must not use Beskid source diagnostics: {diagnostics:#?}"
        );
        assert!(fixes.is_empty(), "standalone BSOL does not synthesize Beskid source fixes");
    }

    #[test]
    fn stale_typed_generation_fails_closed_to_structural_diagnostics() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("compiler workspace root")
            .to_path_buf();
        let _fixture = crate::SHARED_FIXTURE_LOCK.blocking_lock();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");

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
        let (collected, _fixes) = collect_syntax_diagnostics(Some(&mut db), &uri, &source, Some(&ctx));
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
            source: "beskid".to_string(),
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
            assembly: Some(ProgramAssembly::new(
                roots,
                Arc::new(units),
                entry,
                AssemblyDiscovery::ImportClosure,
                index,
                false,
                generation,
            )),
        };
        (directory, resolved)
    }

    /// Design slice 6: the prepare spine reports a dependency unit's legality finding against that
    /// unit's source. The LSP publishes it for the dependency's own URI (when that unit is the
    /// open buffer) and never at the entry buffer's offsets.
    #[test]
    fn dependency_legality_finding_is_published_only_for_its_own_unit() {
        let (_main_dir, main_entry) = dependency_assembly_input(0, 511);
        let main = super::prepare_project_diagnostics_from_assembled(
            &main_entry,
            beskid_analysis::services::DependencyTypingPolicy::FullClosure,
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
            beskid_analysis::services::DependencyTypingPolicy::FullClosure,
        )
        .expect("helper diagnostics");
        let found = helper.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1201")).count();
        assert_eq!(found, 1, "the dependency's own publish carries its E1201 once: {:#?}", helper.diagnostics);
    }
}
