//! The editor's project preparation: the prepare spine for project-backed buffers, the
//! structural fallback, and the facts for a preparation that failed outright.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::CompilationContext;
use beskid_analysis::analysis::diagnostic_codes;
use beskid_analysis::analysis::diagnostics::source_map::SourceMapper;
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions, ResolvedInput, resolved_input_from_plan};
use beskid_analysis::syntax::Program;
use beskid_analysis::{SemanticDiagnostic, Severity, SyntaxFix};
use beskid_queries::BeskidDatabase;
use beskid_tools::native_mods::ModHostSelection;
use tower_lsp_server::ls_types::*;

use super::manifest::{analyze_project_manifest, analyze_standalone_bsol};
use super::to_lsp::{
    DiagnosticDetail, RelatedFact, RelatedPlace, file_uri, first_line, lsp_diagnostics_from_syntax,
    syntax_diagnostic_from_semantic,
};
use crate::manifest_uri::{is_manifest_uri, is_standalone_bsol_uri};
use crate::position::offset_range_to_lsp;
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
        let mut buffer = BufferNames::new(uri);
        let (local, foreign): (Vec<_>, Vec<_>) = structured
            .into_iter()
            .map(|diagnostic| mapper.map_diagnostic(diagnostic))
            .partition(|diagnostic| buffer.names(diagnostic.file_name()));
        let mut facts =
            local.into_iter().map(syntax_diagnostic_from_semantic).filter(|fact| !covered(fact)).collect::<Vec<_>>();
        if let Some(first) = foreign.into_iter().find(|diagnostic| diagnostic.severity == Severity::Error) {
            let (start, end) = first.range();
            let mut fact = SyntaxDiagnostic {
                start: 0,
                end: 0,
                severity: SyntaxDiagnosticSeverity::Error,
                code: first.code.clone(),
                message: format!("project preparation failed in another file: {}", first.message),
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
        code: Some(diagnostic_codes::project_files::PREPARATION_FAILED.to_string()),
        message: format!("project preparation failed: {}", first_line(&error.to_string())),
        source: "beskid".to_string(),
        detail: DiagnosticDetail::default(),
    }]
}

/// The names one buffer goes by: its URI, its path and its canonical path (resolved once).
/// Diagnostic file names that need canonicalizing are resolved once each.
struct BufferNames {
    uri: String,
    path: Option<std::path::PathBuf>,
    canonical: Option<std::path::PathBuf>,
    resolved: std::collections::HashMap<String, bool>,
}

impl BufferNames {
    fn new(uri: &Uri) -> Self {
        let path = uri.to_file_path().map(|path| path.into_owned());
        let canonical = path.as_ref().and_then(|path| path.canonicalize().ok());
        Self { uri: uri.to_string(), path, canonical, resolved: std::collections::HashMap::new() }
    }

    /// Whether a diagnostic file name (a path or a `file:` URI string) names this buffer.
    fn names(&mut self, name: &str) -> bool {
        if name == self.uri || self.path.as_deref().is_some_and(|path| path == std::path::Path::new(name)) {
            return true;
        }
        let Some(canonical) = self.canonical.as_deref() else { return false };
        *self
            .resolved
            .entry(name.to_string())
            .or_insert_with(|| std::path::Path::new(name).canonicalize().is_ok_and(|resolved| resolved == canonical))
    }
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
    let diagnostics = with_recovered_parse_diagnostics(&assembly, diagnostics);
    Ok(PreparedSyntaxFacts {
        diagnostics: entry_buffer_diagnostics(&assembly, diagnostics).map(syntax_diagnostic_from_semantic).collect(),
        assembly,
        fixes,
    })
}

/// One complete preparation of every source under a project's host root (with the dependency
/// modules they import), for the workspace index.
/// The diagnostics keep the file each belongs to, so one typing of the project yields the
/// diagnostics of all its closed files.
pub(crate) struct PreparedProjectIndex {
    pub(crate) assembly: ProgramAssembly,
    pub(crate) diagnostics: Vec<(String, SyntaxDiagnostic)>,
}

pub(crate) fn prepare_project_index(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    mod_host: &ModHostSelection,
) -> anyhow::Result<PreparedProjectIndex> {
    let front_end = FrontEndOptions {
        assembly_discovery: beskid_analysis::projects::AssemblyDiscovery::HostImportClosure,
        ..editor_front_end(EditorPreparePass::Complete)
    };
    let (prepared, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics_with_db(
        db,
        resolved,
        PrepareOptions {
            mod_host: editor_mod_host(resolved, &front_end, mod_host)?,
            front_end,
            native_mod_adapter_sources: false,
        },
        None,
    )?;
    let assembly = prepared.syntax_assembly();
    let diagnostics = with_recovered_parse_diagnostics(&assembly, diagnostics);
    Ok(PreparedProjectIndex {
        assembly,
        diagnostics: diagnostics
            .into_iter()
            .map(|diagnostic| (diagnostic.file_name().to_string(), syntax_diagnostic_from_semantic(diagnostic)))
            .collect(),
    })
}

/// The names diagnostics use for a unit: its logical name and its authored file name.
pub(crate) fn unit_diagnostic_names(unit: &beskid_analysis::projects::SourceUnit) -> Vec<String> {
    let mut mapper = SourceMapper::new();
    let authored = mapper.authored_name(&unit.logical_name);
    std::iter::once(unit.logical_name.clone()).chain(authored).collect()
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
    let diagnostics = with_recovered_parse_diagnostics(&assembly, diagnostics);
    Ok(PreparedSyntaxFacts {
        diagnostics: entry_buffer_diagnostics(&assembly, diagnostics).map(syntax_diagnostic_from_semantic).collect(),
        assembly,
        fixes,
    })
}

/// The parse diagnostics of the assembly's recovered units, then `diagnostics`. An editor assembly
/// keeps the repaired tree of a source that does not parse, so completion and navigation still
/// work; the parse error itself is published here, the same one `beskid check` reports. The
/// recovery is remembered per source, so this parse does not run the recovery again.
fn with_recovered_parse_diagnostics(
    assembly: &ProgramAssembly,
    diagnostics: Vec<SemanticDiagnostic>,
) -> Vec<SemanticDiagnostic> {
    let mut all = assembly
        .units
        .iter()
        .filter(|unit| assembly.recovered_units.contains(&unit.path))
        .flat_map(|unit| {
            match services::parse_program_with_source_name_and_diagnostics(&unit.logical_name, &unit.source) {
                Ok(parsed) => parsed.diagnostics,
                Err(error) => services::structured_diagnostics(&error).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    all.extend(diagnostics);
    all
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
    let mut mapper = SourceMapper::new();
    let foreign = assembly
        .units
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != assembly.entry_index)
        .flat_map(|(_, unit)| {
            let authored = mapper.authored_name(&unit.logical_name);
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

pub(super) fn structural_syntax_diagnostics(source_name: &str, source: &str) -> Vec<SyntaxDiagnostic> {
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
                code: Some(diagnostic_codes::parse::SYNTAX.to_string()),
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
