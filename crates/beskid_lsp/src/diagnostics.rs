//! Map Beskid parse/semantic/manifest errors to generation-bound syntax diagnostic facts
//! and LSP [`Diagnostic`] values.
//!
//! Production publish/refresh paths consume [`SyntaxDiagnostic`] facts attached to the
//! current buffer revision. They never read `Document.analysis` or HIR snapshots.
//!
//! Editor shape (ticket 03, "B plus C"): one diagnostic per cause at its exact span, its message
//! the cause sentence plus at most one `Help:` line, `code` with a `codeDescription` link to the
//! registry page, `source` `beskid`, labelled `relatedInformation`, tags, and its fixes in `data`
//! ([`quick_fixes_from_data`] turns them into quick fixes). Each related place in the buffer also
//! gets a Hint marker with the same code, the label as message and a link back to the cause. The
//! compiler has already mapped spans to the authored source; the editor does no mapping.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::projects::{ProjectError, parse_bsol_document, parse_manifest, parse_workspace_manifest};
use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions, ResolvedInput, resolved_input_from_plan};
use beskid_analysis::syntax::Program;
use beskid_analysis::analysis::diagnostic_codes;
use beskid_analysis::analysis::diagnostics::source_map::SourceMapper;
use beskid_analysis::{Applicability, DiagnosticTag as BeskidTag, SemanticDiagnostic, Severity, SyntaxFix};
use beskid_queries::BeskidDatabase;
use beskid_tools::native_mods::ModHostSelection;
use tower_lsp_server::ls_types::*;

use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::{is_manifest_uri, is_standalone_bsol_uri};
use crate::position::offset_range_to_lsp;
use crate::workspace_scan::path_to_uri;
use crate::session::store::{SyntaxDiagnostic, SyntaxDiagnosticSeverity};

/// Qualified native Mod execution for editor diagnostics. The editor never builds Mods: it runs
/// the artifact the CLI cached or the toolchain shipped in the session's Mod host and runtime kit,
/// or reports the exact rebuild command.
///
/// Only the Mods this buffer's assembly activates are qualified, decided on the same editor
/// assembly policy the preparation uses.
fn editor_mod_host(
    resolved: &ResolvedInput,
    front_end: &FrontEndOptions,
    mod_host: &ModHostSelection,
) -> anyhow::Result<beskid_analysis::mod_host::ModHost> {
    match resolved.compile_plan.as_ref() {
        Some(_) => mod_host.mod_host_for_input(resolved, front_end),
        None => Ok(beskid_analysis::mod_host::ModHost::NoMods),
    }
}

/// Which of the editor's two preparation passes runs. Both run the one query gate; they differ
/// only in scheduling. The immediate pass after an edit prepares the buffer without collecting
/// the structural rule diagnostics, so completion and navigation facts refresh at once; the
/// complete pass (debounced, on open, and after invalidation) collects every diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorPreparePass {
    Immediate,
    Complete,
}

fn editor_front_end(pass: EditorPreparePass) -> FrontEndOptions {
    FrontEndOptions {
        assembly_recovery: beskid_analysis::projects::AssemblyRecoveryPolicy::EditorRetainRecovered,
        with_semantic_diagnostics: pass == EditorPreparePass::Complete,
        ..Default::default()
    }
}

/// The session's Salsa database and native Mod host, which together prepare a project-backed
/// buffer.
pub struct ProjectPreparation<'a> {
    pub db: &'a mut BeskidDatabase,
    pub mod_host: &'a ModHostSelection,
}

/// Facts for a project preparation that failed outright (for example a native Mod without a
/// current artifact), so the failure is visible instead of silently degrading to structural facts.
///
/// A failure that carries structured diagnostics is reported through them: those in this buffer at
/// their exact spans, those in other files as one fact linking to the first of them. A cause the
/// buffer's own facts in `reported` already show (same code and span) is not reported again, and
/// an unstructured failure is reported only when no file-level error explains it.
pub(crate) fn project_preparation_failure(
    uri: &Uri,
    error: &anyhow::Error,
    reported: &[SyntaxDiagnostic],
) -> Vec<SyntaxDiagnostic> {
    let covered = |fact: &SyntaxDiagnostic| {
        reported.iter().any(|known| known.code == fact.code && known.start == fact.start && known.end == fact.end)
    };
    if let Some(structured) = services::structured_diagnostics(error) {
        let mut mapper = SourceMapper::new();
        let (local, foreign): (Vec<_>, Vec<_>) = structured
            .into_iter()
            .map(|diagnostic| mapper.map_diagnostic(diagnostic))
            .partition(|diagnostic| names_buffer(uri, diagnostic.file_name()));
        let mut facts =
            local.into_iter().map(syntax_diagnostic_from_semantic).filter(|fact| !covered(fact)).collect::<Vec<_>>();
        if let Some(first) = foreign.into_iter().find(|diagnostic| diagnostic.severity == Severity::Error) {
            let (start, end) = first.range();
            let mut fact = SyntaxDiagnostic {
                start: 0,
                end: 0,
                severity: SyntaxDiagnosticSeverity::Error,
                code: first.code.clone(),
                message: format!("Project preparation failed in another file: {}", first.message),
                source: "beskid".to_string(),
                detail: DiagnosticDetail { uri: Some(uri.to_string()), ..DiagnosticDetail::default() },
            };
            if let Some(file) = file_uri(first.file_name()) {
                fact.detail.related.push(RelatedFact {
                    label: first.label.clone(),
                    place: RelatedPlace::Foreign {
                        uri: file,
                        range: offset_range_to_lsp(first.source_text(), start, end),
                    },
                });
            }
            facts.push(fact);
        }
        return facts;
    }
    if reported.iter().any(|fact| fact.severity == SyntaxDiagnosticSeverity::Error) {
        return Vec::new();
    }
    vec![SyntaxDiagnostic {
        start: 0,
        end: 0,
        severity: SyntaxDiagnosticSeverity::Error,
        code: Some("project-preparation".to_string()),
        message: format!("Project preparation failed: {}", first_line(&error.to_string())),
        source: "beskid".to_string(),
        detail: DiagnosticDetail::default(),
    }]
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default().trim()
}

/// Whether a diagnostic file name (a path or a `file:` URI string) names the buffer at `uri`.
fn names_buffer(uri: &Uri, name: &str) -> bool {
    if name == uri.as_str() {
        return true;
    }
    let Some(path) = uri.to_file_path() else { return false };
    let (name, path) = (std::path::Path::new(name), path.as_ref());
    name == path || (name.canonicalize().ok().zip(path.canonicalize().ok())).is_some_and(|(left, right)| left == right)
}

/// The `file:` URI string for a diagnostic file name (a URI string or an absolute path).
fn file_uri(name: &str) -> Option<String> {
    if name.starts_with("file:") {
        return Some(name.to_string());
    }
    let path = std::path::Path::new(name);
    path.is_absolute().then(|| path_to_uri(path)).flatten().map(|uri| uri.to_string())
}

/// Help, related locations, fixes and tags of one editor diagnostic. Buffer places are byte
/// offsets into the buffer the fact belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticDetail {
    pub help: Option<String>,
    /// `file:` URI of the buffer, the target of related information in the buffer.
    pub uri: Option<String>,
    pub related: Vec<RelatedFact>,
    pub fixes: Vec<FixFact>,
    pub tags: Vec<BeskidTag>,
}

/// A labelled related place of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedFact {
    pub label: String,
    pub place: RelatedPlace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelatedPlace {
    /// Byte range in the diagnostic's own buffer.
    Buffer { start: usize, end: usize },
    /// A range in another file.
    Foreign { uri: String, range: Range },
}

/// One quick fix with its edits as byte ranges in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixFact {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<(usize, usize, String)>,
}

/// One fix as it travels in `Diagnostic.data` between publish and `textDocument/codeAction`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixData {
    title: String,
    applicability: String,
    edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DiagnosticData {
    fixes: Vec<FixData>,
}

/// Quick fixes for one diagnostic the client sent back with its `data`: every fix the compiler
/// attached, preferred only when machine-applicable, with a title that says to check the result
/// otherwise.
pub(crate) fn quick_fixes_from_data(uri: &Uri, diagnostic: &Diagnostic) -> Vec<CodeAction> {
    let Some(data) =
        diagnostic.data.clone().and_then(|data| serde_json::from_value::<DiagnosticData>(data).ok())
    else {
        return Vec::new();
    };
    data.fixes
        .into_iter()
        .filter_map(|fix| {
            let applicability = Applicability::parse(&fix.applicability)?;
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), fix.edits);
            Some(CodeAction {
                title: fix.title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
                is_preferred: Some(applicability == Applicability::MachineApplicable),
                ..CodeAction::default()
            })
        })
        .collect()
}

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
    pass: EditorPreparePass,
    mod_host: &ModHostSelection,
) -> anyhow::Result<PreparedSyntaxFacts> {
    let (prepared, diagnostics, fixes) = beskid_queries::prepare_compilation_diagnostics_with_db(
        db,
        resolved,
        PrepareOptions {
            mod_host: editor_mod_host(resolved, &editor_front_end(pass), mod_host)?,
            front_end: editor_front_end(pass),
            native_mod_adapter_sources: false,
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
    pass: EditorPreparePass,
    mod_host: &ModHostSelection,
) -> anyhow::Result<PreparedSyntaxFacts> {
    let (prepared, diagnostics, fixes) = beskid_queries::prepare_compilation_diagnostics_isolated(
        resolved,
        PrepareOptions {
            mod_host: editor_mod_host(resolved, &editor_front_end(pass), mod_host)?,
            front_end: editor_front_end(pass),
            native_mod_adapter_sources: false,
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
/// (where it is the entry) and never to this buffer's offsets. Diagnostics already name authored
/// files, so a unit's authored name counts as well as its logical name.
fn entry_buffer_diagnostics(
    assembly: &ProgramAssembly,
    diagnostics: Vec<SemanticDiagnostic>,
) -> impl Iterator<Item = SemanticDiagnostic> {
    let foreign = assembly
        .units
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != assembly.entry_index)
        .flat_map(|(_, unit)| {
            let authored = SourceMapper::new().authored_name(&unit.logical_name);
            std::iter::once(unit.logical_name.clone()).chain(authored)
        })
        .collect::<std::collections::HashSet<_>>();
    diagnostics.into_iter().filter(move |diagnostic| !foreign.contains(diagnostic.file_name()))
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
    project: Option<ProjectPreparation<'_>>,
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
        && let (Some(ProjectPreparation { db, mod_host }), Some(ctx)) = (project, compilation_context)
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
        return match prepare_project_syntax_facts(db, &resolved, EditorPreparePass::Complete, mod_host) {
            Ok(prepared) => (prepared.diagnostics, prepared.fixes),
            Err(error) => {
                tracing::warn!(uri = %uri.as_str(), "project preparation failed: {error:#}");
                let mut diagnostics = structural_syntax_diagnostics(&uri.to_string(), source);
                let failure = project_preparation_failure(uri, &error, &diagnostics);
                diagnostics.extend(failure);
                (diagnostics, Vec::new())
            }
        };
    }

    (structural_syntax_diagnostics(&uri.to_string(), source), Vec::new())
}

/// Convert generation-bound facts into LSP diagnostics for the given source text.
///
/// Each fact becomes one diagnostic plus one Hint marker per related place in the buffer.
pub fn lsp_diagnostics_from_syntax(source: &str, facts: &[SyntaxDiagnostic]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::with_capacity(facts.len());
    for fact in facts {
        syntax_to_lsp_diagnostics(source, fact, &mut diagnostics);
    }
    diagnostics
}

/// Produce LSP diagnostics for a buffer (one-shot callers / tests).
///
/// Prefer [`collect_syntax_diagnostics`] + [`lsp_diagnostics_from_syntax`] for publish paths
/// that already hold generation-bound Document facts.
#[cfg_attr(not(test), allow(dead_code))]
pub fn analyze_document(
    project: Option<ProjectPreparation<'_>>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<Diagnostic> {
    let (facts, _fixes) = collect_syntax_diagnostics(project, uri, source, compilation_context);
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
        // A parse failure carries its structured diagnostic, reported at its exact span.
        Err(err) => match services::structured_diagnostics(&err) {
            Some(structured) => structured.into_iter().map(syntax_diagnostic_from_semantic).collect(),
            None => vec![SyntaxDiagnostic {
                start: 0,
                end: 0,
                severity: SyntaxDiagnosticSeverity::Error,
                code: Some("parse".to_string()),
                message: first_line(&err.to_string()).to_string(),
                source: "beskid".to_string(),
                detail: DiagnosticDetail::default(),
            }],
        },
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
    let (start, end) = diag.range();
    let end = end.max(start.saturating_add(1));
    let related = diag
        .related
        .iter()
        .filter_map(|related| {
            let (start, end) = (related.span.offset(), related.span.offset() + related.span.len());
            let place = match &related.source {
                None => RelatedPlace::Buffer { start, end },
                Some(source) => RelatedPlace::Foreign {
                    uri: file_uri(&source.name)?,
                    range: offset_range_to_lsp(&source.text, start, end),
                },
            };
            Some(RelatedFact { label: related.label.clone(), place })
        })
        .collect();
    let fixes = diag
        .fixes
        .iter()
        .map(|fix| FixFact {
            title: fix.editor_title(),
            applicability: fix.applicability,
            edits: fix
                .edits
                .iter()
                .map(|edit| {
                    let (start, end) = edit.range();
                    (start, end, edit.replacement.clone())
                })
                .collect(),
        })
        .collect();
    let detail = DiagnosticDetail {
        help: diag.help.clone(),
        uri: file_uri(diag.file_name()),
        related,
        fixes,
        tags: diag.tags.clone(),
    };
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
        detail,
    }
}

/// The editor message: the cause sentence and, on the cause only, one `Help:` line.
fn editor_message(fact: &SyntaxDiagnostic) -> String {
    match fact.detail.help.as_deref().map(first_line).filter(|help| !help.is_empty()) {
        Some(help) => format!("{}\nHelp: {help}", first_line(&fact.message)),
        None => first_line(&fact.message).to_string(),
    }
}

fn syntax_to_lsp_diagnostics(source: &str, fact: &SyntaxDiagnostic, out: &mut Vec<Diagnostic>) {
    let range = offset_range_to_lsp(source, fact.start, fact.end.max(fact.start.saturating_add(1)));
    let code = fact.code.clone().map(NumberOrString::String);
    let code_description = fact
        .code
        .as_deref()
        .and_then(diagnostic_codes::docs_url)
        .and_then(|href| href.parse::<Uri>().ok())
        .map(|href| CodeDescription { href });
    let buffer_uri = fact.detail.uri.as_deref().and_then(|uri| uri.parse::<Uri>().ok());
    let buffer_range = |start: usize, end: usize| offset_range_to_lsp(source, start, end.max(start));
    let related_information = fact
        .detail
        .related
        .iter()
        .filter_map(|related| {
            let location = match &related.place {
                RelatedPlace::Buffer { start, end } => {
                    Location { uri: buffer_uri.clone()?, range: buffer_range(*start, *end) }
                }
                RelatedPlace::Foreign { uri, range } => Location { uri: uri.parse().ok()?, range: *range },
            };
            Some(DiagnosticRelatedInformation { location, message: related.label.clone() })
        })
        .collect::<Vec<_>>();
    let tags = fact
        .detail
        .tags
        .iter()
        .map(|tag| match tag {
            BeskidTag::Unnecessary => DiagnosticTag::UNNECESSARY,
            BeskidTag::Deprecated => DiagnosticTag::DEPRECATED,
        })
        .collect::<Vec<_>>();
    let data = (!fact.detail.fixes.is_empty()).then(|| {
        let fixes = fact
            .detail
            .fixes
            .iter()
            .map(|fix| FixData {
                title: fix.title.clone(),
                applicability: fix.applicability.as_str().to_string(),
                edits: fix
                    .edits
                    .iter()
                    .map(|(start, end, text)| TextEdit { range: buffer_range(*start, *end), new_text: text.clone() })
                    .collect(),
            })
            .collect();
        serde_json::to_value(DiagnosticData { fixes }).expect("diagnostic data serializes")
    });
    let source_tag = Some(fact.source.clone());
    out.push(Diagnostic {
        range,
        severity: Some(match fact.severity {
            SyntaxDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            SyntaxDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
            SyntaxDiagnosticSeverity::Note => DiagnosticSeverity::INFORMATION,
        }),
        code: code.clone(),
        code_description: code_description.clone(),
        source: source_tag.clone(),
        message: editor_message(fact),
        related_information: (!related_information.is_empty()).then_some(related_information),
        tags: (!tags.is_empty()).then_some(tags),
        data,
    });
    // Light markers: a Hint at each related place in this buffer, pointing back to the cause.
    for related in &fact.detail.related {
        let RelatedPlace::Buffer { start, end } = related.place else { continue };
        let back = buffer_uri.clone().map(|uri| {
            vec![DiagnosticRelatedInformation {
                location: Location { uri, range },
                message: first_line(&fact.message).to_string(),
            }]
        });
        out.push(Diagnostic {
            range: buffer_range(start, end),
            severity: Some(DiagnosticSeverity::HINT),
            code: code.clone(),
            code_description: code_description.clone(),
            source: source_tag.clone(),
            message: related.label.clone(),
            related_information: back,
            ..Diagnostic::default()
        });
    }
}

fn analyze_project_manifest(uri: &Uri, source: &str) -> Vec<SyntaxDiagnostic> {
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
        let mod_host = beskid_tools::native_mods::ModHostSelection::installed();
        let (collected, _fixes) = collect_syntax_diagnostics(
            Some(super::ProjectPreparation { db: &mut db, mod_host: &mod_host }),
            &uri,
            &source,
            Some(&ctx),
        );
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

    /// Design slice 6: the prepare spine reports a dependency unit's legality finding against that
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
        .with_fix(Fix::new("Make `n` mutable", Applicability::MachineApplicable, vec![FixEdit::insert(declaration, "mut ")]))
        .with_fix(Fix::replace("Rename the second `n`", span(assignment, assignment + 1), "m", Applicability::MaybeIncorrect))
        .with_tag(beskid_analysis::DiagnosticTag::Deprecated);
        super::syntax_diagnostic_from_semantic(diagnostic)
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
        let actions = super::quick_fixes_from_data(&uri, &diagnostics[0]);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].title, "Make `n` mutable");
        assert_eq!(actions[0].is_preferred, Some(true));
        let edits = &actions[0].edit.as_ref().expect("edit").changes.as_ref().expect("changes")[&uri];
        assert_eq!(edits[0].new_text, "mut ");
        assert_eq!(edits[0].range.start, edits[0].range.end, "an insertion");
        assert_eq!(actions[1].title, "Rename the second `n` (check the result)");
        assert_eq!(actions[1].is_preferred, Some(false));
        assert!(super::quick_fixes_from_data(&uri, &diagnostics[1]).is_empty(), "markers carry no fixes");
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
        let diagnostics = lsp_diagnostics_from_syntax("use A.B;\n", &[super::syntax_diagnostic_from_semantic(diagnostic)]);
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
        let error = anyhow::Error::new(beskid_analysis::MietteReportError::new(structured.clone()))
            .context("project preparation");
        let reported = vec![super::syntax_diagnostic_from_semantic(structured)];
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
        assert_eq!(facts[0].message, "Project preparation failed: Mod artifact is stale");
        let file_error = SyntaxDiagnostic { start: 1, end: 2, ..facts[0].clone() };
        assert!(super::project_preparation_failure(&uri, &error, &[file_error]).is_empty());
    }
}
