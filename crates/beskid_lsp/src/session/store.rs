use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use beskid_analysis::CompilationContext;
// Re-export the single host-side `SyntaxFix` / `SyntaxTextEdit` implementation so LSP
// code can name them as `crate::session::store::SyntaxFix` / `SyntaxTextEdit` (DRY — no
// duplicate LSP-side types). Defined in `beskid_analysis::mod_host::diagnostics` and
// returned by the prepare spine. `SyntaxTextEditKind` is consumed only by tests, which
// import it directly from `beskid_analysis`.
pub use beskid_analysis::{SyntaxFix, SyntaxTextEdit};
use beskid_queries::AstNodeKey;
use beskid_tools::native_mods::ModHostSelection;
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tower_lsp_server::ls_types::Uri;

use super::{documentation_facts::SyntaxDocumentationFact, project_dbs::ProjectDatabases};

/// One editor buffer or disk snapshot with generation-bound syntax facts.
#[derive(Debug, Clone)]
pub struct Document {
    pub version: i32,
    pub text: String,
    /// Generation-safe syntax/Salsa definition facts for this exact buffer revision.
    ///
    /// Definition handling consumes this index instead of reaching back into the legacy
    /// HIR-backed analysis snapshot.
    pub syntax_definitions: Vec<SyntaxDefinition>,
    /// Generation-safe syntax/Salsa hover facts for this exact buffer revision.
    pub syntax_hovers: Vec<SyntaxHover>,
    pub syntax_symbols: Vec<SyntaxSymbol>,
    /// Structural BSOL token candidates parsed once for this exact buffer revision.
    pub bsol_semantic_token_candidates: Vec<BsolSemanticTokenCandidate>,
    /// Attribute names of the current buffer's recoverable syntax (semantic tokens).
    pub syntax_attribute_names: Vec<SyntaxAttributeName>,
    /// Generation-bound root key used by syntax-only completion queries.
    pub syntax_completion: Option<SyntaxCompletion>,
    /// Generation-safe type facts for source nodes in this exact buffer revision.
    pub syntax_inlay_hints: Vec<SyntaxInlayHint>,
    /// Generation-bound declaration/documentation shape for this exact buffer revision.
    pub syntax_documentation: Vec<SyntaxDocumentationFact>,
    /// Generation-bound diagnostics for this exact buffer revision (publish/refresh authority).
    pub syntax_diagnostics: Vec<SyntaxDiagnostic>,
    /// Generation-bound mod-origin quick-fixes for this exact buffer revision. Each fix
    /// links to a `syntax_diagnostics` entry via `(source, code)` and is surfaced by the
    /// `ModQuickFixProvider` code-action registry.
    pub syntax_fixes: Vec<SyntaxFix>,
}

impl Document {
    /// Drop generation-bound syntax facts after hard invalidation (fail closed until rebuild).
    pub fn clear_syntax_facts(&mut self) {
        self.syntax_definitions.clear();
        self.syntax_hovers.clear();
        self.syntax_symbols.clear();
        // BSOL candidates depend only on this document's immutable text generation,
        // not on the invalidated Beskid compilation context.
        self.syntax_completion = None;
        self.syntax_inlay_hints.clear();
        self.syntax_documentation.clear();
        self.syntax_diagnostics.clear();
        self.syntax_fixes.clear();
    }
}

/// The single edit between two texts: the common prefix and suffix, and the length change.
/// Offsets outside the replaced middle keep their meaning across the edit.
struct TextEdit {
    prefix: usize,
    old_suffix_start: usize,
    delta: isize,
}

impl TextEdit {
    fn between(old: &str, new: &str) -> Self {
        let mut prefix = old.bytes().zip(new.bytes()).take_while(|(left, right)| left == right).count();
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let max_suffix = old.len().min(new.len()) - prefix;
        let mut suffix =
            old.bytes().rev().zip(new.bytes().rev()).take(max_suffix).take_while(|(left, right)| left == right).count();
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
            suffix -= 1;
        }
        Self { prefix, old_suffix_start: old.len() - suffix, delta: new.len() as isize - old.len() as isize }
    }

    /// The span `[start, end]` of the old text in the new text, when it lies outside the edit.
    fn span(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        if end <= self.prefix {
            Some((start, end))
        } else if start >= self.old_suffix_start {
            Some(((start as isize + self.delta) as usize, (end as isize + self.delta) as usize))
        } else {
            None
        }
    }
}

impl Document {
    /// This document's facts carried over to `text`, the newer text of the same buffer whose
    /// analysis has not finished. Facts outside the edited region move with their text; facts
    /// inside it, diagnostics and everything bound to the analyzed generation (the completion
    /// anchor, inlay hints) are dropped. Hover, definition and completion answer from this
    /// instead of waiting for the analysis of every keystroke.
    pub(crate) fn carried_over(&self, text: String, path: Option<&std::path::Path>) -> Document {
        let edit = TextEdit::between(&self.text, &text);
        let syntax_hovers = self
            .syntax_hovers
            .iter()
            .filter_map(|hover| {
                let (reference_start, reference_end) = edit.span(hover.reference_start, hover.reference_end)?;
                let (location_start, location_end) = if path == Some(hover.location_path.as_path()) {
                    edit.span(hover.location_start, hover.location_end)?
                } else {
                    (hover.location_start, hover.location_end)
                };
                Some(SyntaxHover { reference_start, reference_end, location_start, location_end, ..hover.clone() })
            })
            .collect();
        let syntax_completion = self.syntax_completion.as_ref().map(|completion| SyntaxCompletion {
            entry_anchor: None,
            dependency_surface: Arc::clone(&completion.dependency_surface),
            attributes: Arc::clone(&completion.attributes),
            member_access: completion
                .member_access
                .iter()
                .filter_map(|(offset, candidates)| {
                    edit.span(*offset, *offset).map(|(offset, _)| (offset, Arc::clone(candidates)))
                })
                .collect::<Vec<_>>()
                .into(),
        });
        let syntax_definitions = self
            .syntax_definitions
            .iter()
            .filter_map(|definition| {
                let (reference_start, reference_end) =
                    edit.span(definition.reference_start, definition.reference_end)?;
                let (declaration_start, declaration_end) = if path == Some(definition.declaration_path.as_path()) {
                    edit.span(definition.declaration_start, definition.declaration_end)?
                } else {
                    (definition.declaration_start, definition.declaration_end)
                };
                Some(SyntaxDefinition {
                    reference_start,
                    reference_end,
                    declaration_start,
                    declaration_end,
                    ..definition.clone()
                })
            })
            .collect();
        let syntax_symbols = self
            .syntax_symbols
            .iter()
            .filter_map(|symbol| {
                let (start, end) = edit.span(symbol.start, symbol.end)?;
                Some(SyntaxSymbol { start, end, ..symbol.clone() })
            })
            .collect();
        Document {
            version: self.version,
            text,
            syntax_definitions,
            syntax_hovers,
            syntax_symbols,
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BsolSemanticTokenCandidate {
    pub start: usize,
    pub end: usize,
    pub kind: BsolSemanticTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsolSemanticTokenKind {
    Namespace,
    Property,
}

/// One diagnostic proven for the current buffer revision (never an analysis/HIR snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    pub start: usize,
    pub end: usize,
    pub severity: SyntaxDiagnosticSeverity,
    pub code: Option<String>,
    pub message: String,
    /// Origin tag for code-action routing. `"beskid"` for compiler diagnostics;
    /// `"beskid:mod:<type_id>"` for mod-origin diagnostics produced by a native
    /// `Analyzer` contract. Mirrors `SemanticDiagnostic.origin`.
    pub source: String,
    /// Help, related locations, fixes and tags of the structured diagnostic.
    pub detail: crate::diagnostics::DiagnosticDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxDiagnosticSeverity {
    Error,
    Warning,
    Note,
}

/// One resolved syntax reference and its declaration location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDefinition {
    pub reference_start: usize,
    pub reference_end: usize,
    pub declaration_path: PathBuf,
    pub declaration_start: usize,
    pub declaration_end: usize,
}

/// Markdown hover content and the declaration span it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxHover {
    pub reference_start: usize,
    pub reference_end: usize,
    pub markdown: String,
    pub location_path: PathBuf,
    pub location_start: usize,
    pub location_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxSymbol {
    pub name: String,
    pub kind: beskid_analysis::services::AnalysisSymbolKind,
    pub start: usize,
    pub end: usize,
}

/// One source span whose type was proven by a generation-safe semantic query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxInlayHint {
    pub start: usize,
    pub end: usize,
    pub type_label: String,
}

/// The authoritative syntax generation available to a completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxCompletion {
    /// Current-buffer syntax authority when the fast prepare accepted the edit.
    pub entry_anchor: Option<AstNodeKey>,
    /// Owned imported-member surface captured from this document's prepared
    /// assembly. A newer partial snapshot may retain only bindings still present
    /// in its recoverable imports; the surface is never broader semantic authority.
    pub dependency_surface: Arc<[beskid_queries::CompletionMemberSurface]>,
    /// Owned attribute catalogue resolved from the entry unit of the prepared assembly
    /// (`resolve_attribute_use`). Declarations in the current buffer take precedence at
    /// request time, matching resolution step 1.
    pub attributes: Arc<[AttributeCompletionEntry]>,
    /// Owned member candidates of each member access that ends its line in the buffer (`String.`
    /// or `point.` while the member is typed), keyed by the offset just after the dot. Captured
    /// from the buffer's completion repair when its facts were built, so a later registration of
    /// the same unit (the workspace scan, another buffer) cannot take them away.
    pub member_access: Arc<[(usize, Arc<[beskid_queries::CompletionCandidate]>)]>,
}

/// One attribute visible from the entry unit with the target list E1510 checks against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeCompletionEntry {
    pub name: Arc<str>,
    pub targets: Arc<[Arc<str>]>,
    pub recognized: bool,
}

/// One attribute name in the current buffer (a use at any declaration site, or the declared
/// name of an attribute declaration when `declaration` is set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxAttributeName {
    pub start: usize,
    pub end: usize,
    pub declaration: bool,
}

/// Full-diagnostics preparations that run at once (see [`State::diagnostics_permits`]).
pub(crate) const MAX_CONCURRENT_DIAGNOSTICS: usize = 2;

/// Project analyses that run at once (see [`State::analysis_permits`]).
pub(crate) const MAX_CONCURRENT_ANALYSES: usize = 2;

/// In-memory LSP workspace: open docs, closed-but-indexed files, and compilation context cache.
pub struct State {
    /// Canonical `.bproj` path from init options or `beskid.focusedProjectUri` configuration.
    pub focused_project: Option<PathBuf>,
    pub docs: HashMap<Uri, Document>,
    /// Closed files on disk that still receive diagnostics (not managed by the editor buffer).
    pub workspace_index: HashMap<Uri, Document>,
    /// Advances whenever project/lock authority is invalidated, even when an editor
    /// buffer's text and LSP version do not change.
    pub(crate) diagnostic_generation: u64,
    /// Key: canonical `.bproj` path.
    pub compilation_context_cache: HashMap<PathBuf, CompilationContext>,
    /// One in-memory Salsa database per project, each with its own mutation gate.
    pub(crate) databases: ProjectDatabases,
    /// Preserves notification order per URI without blocking unrelated buffers.
    pub(crate) document_update_gates: HashMap<Uri, Arc<AsyncMutex<()>>>,
    /// Set after the first workspace scan finishes (gates Salsa work during startup).
    ///
    /// This releases the startup barrier once manifests are indexed; closed sources are still
    /// being analyzed then. [`State::workspace_index_complete`] says when that finished.
    pub(crate) initial_scan_complete: Arc<AtomicBool>,
    /// Set when the first scan has analyzed every closed source. Before that, `workspace_index`
    /// is a partial index: features must treat a missing entry as "not indexed yet".
    pub(crate) workspace_index_complete: Arc<AtomicBool>,
    /// Wakes tasks waiting on [`initial_scan_complete`].
    pub(crate) scan_barrier: Arc<Notify>,
    /// Native Mod host that project preparation runs Mods in.
    ///
    /// Defaults to the installed toolchain (the `beskid` CLI next to this server and its runtime
    /// kit). The `modHost` initialization option selects another one, in the manner of
    /// rust-analyzer's `procMacro.server`; artifact qualification applies to either.
    pub mod_host: ModHostSelection,
    /// Interactive requests currently running; background scans yield while this is non-zero.
    pub(crate) interactive_inflight: Arc<crate::scan_schedule::InteractiveGate>,
    /// Woken whenever an open document's analyzed facts are stored, so requests that need the
    /// facts of the newest text can wait for them.
    pub(crate) facts_stored: Arc<Notify>,
    /// Bounds the full-diagnostics preparations that run at once. Each holds a typed program of
    /// its project (gigabytes for a large Corelib package) outside the project's database gate.
    pub(crate) diagnostics_permits: Arc<tokio::sync::Semaphore>,
    /// Bounds the analyses of open documents (database work of any project) that run at once.
    /// Each holds a project's typed closure, so analyses of many projects at once would hold
    /// many of them.
    pub(crate) analysis_permits: Arc<tokio::sync::Semaphore>,
    /// Closed files analyzed one by one (a closed document, a file changed on disk) run one at a
    /// time on their own permit, so they cannot delay an edit's analysis.
    pub(crate) background_permits: Arc<tokio::sync::Semaphore>,
    /// The workspace index types three projects at a time on its own permits, so neither edits
    /// nor closed files queue in front of it.
    pub(crate) index_permits: Arc<tokio::sync::Semaphore>,
    /// The workspace index's fair share against interactive analyses.
    pub(crate) background_turn: Arc<crate::scan_schedule::BackgroundTurn>,
    /// Content fingerprints of manifests and lockfiles already acted on, so a rewrite with
    /// identical bytes (the server's own `Project.lock` sync) causes no invalidation.
    pub(crate) watched_fingerprints: HashMap<PathBuf, u64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            focused_project: None,
            docs: HashMap::new(),
            workspace_index: HashMap::new(),
            diagnostic_generation: 0,
            compilation_context_cache: HashMap::new(),
            databases: ProjectDatabases::default(),
            document_update_gates: HashMap::new(),
            initial_scan_complete: Arc::new(AtomicBool::new(false)),
            workspace_index_complete: Arc::new(AtomicBool::new(false)),
            scan_barrier: Arc::new(Notify::new()),
            mod_host: ModHostSelection::installed(),
            interactive_inflight: Arc::new(crate::scan_schedule::InteractiveGate::default()),
            watched_fingerprints: HashMap::new(),
            facts_stored: Arc::new(Notify::new()),
            diagnostics_permits: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DIAGNOSTICS)),
            analysis_permits: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_ANALYSES)),
            background_permits: Arc::new(tokio::sync::Semaphore::new(1)),
            index_permits: Arc::new(tokio::sync::Semaphore::new(3)),
            background_turn: Arc::default(),
        }
    }
}

impl State {
    pub fn document_union(&self, uri: &Uri) -> Option<Document> {
        self.docs.get(uri).cloned().or_else(|| self.workspace_index.get(uri).cloned())
    }

    pub(crate) fn mark_workspace_index_complete(&self) {
        self.workspace_index_complete.store(true, Ordering::Release);
    }

    pub(crate) fn mark_initial_scan_complete(&self) {
        self.initial_scan_complete.store(true, Ordering::Release);
        self.scan_barrier.notify_waiters();
    }
}

#[cfg(test)]
mod carry_over_tests {
    use super::*;

    fn hover(start: usize, end: usize, path: &str) -> SyntaxHover {
        SyntaxHover {
            reference_start: start,
            reference_end: end,
            markdown: format!("hover {start}"),
            location_path: PathBuf::from(path),
            location_start: start,
            location_end: end,
        }
    }

    fn document(text: &str, hovers: Vec<SyntaxHover>, member_access: Vec<usize>) -> Document {
        Document {
            version: 1,
            text: text.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: hovers,
            syntax_symbols: Vec::new(),
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: Some(SyntaxCompletion {
                entry_anchor: None,
                dependency_surface: Arc::from([]),
                attributes: Arc::from([]),
                member_access: member_access
                    .into_iter()
                    .map(|offset| (offset, Arc::from([])))
                    .collect::<Vec<_>>()
                    .into(),
            }),
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: vec![SyntaxDiagnostic {
                start: 0,
                end: 1,
                severity: SyntaxDiagnosticSeverity::Error,
                code: None,
                message: "old".into(),
                source: "beskid".into(),
                detail: Default::default(),
            }],
            syntax_fixes: Vec::new(),
        }
    }

    #[test]
    fn facts_before_and_after_an_edit_move_with_their_text_and_facts_inside_it_are_dropped() {
        //          0123456789012345678901
        let old = "alpha beta gamma delta";
        let new = "alpha BETA2 gamma delta"; // `beta` (6..10) became `BETA2` (6..11)
        let path = std::path::Path::new("/w/Main.bd");
        let doc = document(
            old,
            vec![hover(0, 5, "/w/Main.bd"), hover(6, 10, "/w/Main.bd"), hover(11, 16, "/w/Other.bd")],
            vec![5, 8, 17],
        );
        let carried = doc.carried_over(new.to_string(), Some(path));
        assert_eq!(carried.text, new);
        let spans: Vec<_> = carried
            .syntax_hovers
            .iter()
            .map(|h| (h.reference_start, h.reference_end, h.location_start, h.location_end))
            .collect();
        assert_eq!(spans, vec![(0, 5, 0, 5), (12, 17, 11, 16)], "a location in another file keeps its offsets");
        assert_eq!(&new[12..17], "gamma");
        let offsets: Vec<_> =
            carried.syntax_completion.as_ref().unwrap().member_access.iter().map(|(offset, _)| *offset).collect();
        assert_eq!(offsets, vec![5, 18]);
        assert!(carried.syntax_diagnostics.is_empty(), "diagnostics belong to the analyzed text");
    }

    #[test]
    fn definitions_move_with_their_text_like_hovers() {
        //          0123456789012345678901
        let old = "alpha beta gamma delta";
        let new = "alpha BETA2 gamma delta";
        let path = std::path::Path::new("/w/Main.bd");
        let definition = |start: usize, end: usize, declaration: &str| SyntaxDefinition {
            reference_start: start,
            reference_end: end,
            declaration_path: PathBuf::from(declaration),
            declaration_start: start,
            declaration_end: end,
        };
        let mut doc = document(old, Vec::new(), Vec::new());
        doc.syntax_definitions =
            vec![definition(0, 5, "/w/Main.bd"), definition(6, 10, "/w/Main.bd"), definition(11, 16, "/w/Other.bd")];
        let carried = doc.carried_over(new.to_string(), Some(path));
        let spans: Vec<_> = carried
            .syntax_definitions
            .iter()
            .map(|d| (d.reference_start, d.reference_end, d.declaration_start, d.declaration_end))
            .collect();
        assert_eq!(spans, vec![(0, 5, 0, 5), (12, 17, 11, 16)], "a declaration in another file keeps its offsets");
    }

    #[test]
    fn outline_symbols_move_with_their_text() {
        let old = "alpha beta gamma delta";
        let new = "alpha BETA2 gamma delta";
        let symbol = |name: &str, start: usize, end: usize| SyntaxSymbol {
            name: name.to_string(),
            kind: beskid_analysis::services::AnalysisSymbolKind::Function,
            start,
            end,
        };
        let mut doc = document(old, Vec::new(), Vec::new());
        doc.syntax_symbols = vec![symbol("alpha", 0, 5), symbol("beta", 6, 10), symbol("gamma", 11, 16)];
        let carried = doc.carried_over(new.to_string(), None);
        let spans: Vec<_> = carried.syntax_symbols.iter().map(|s| (s.name.as_str(), s.start, s.end)).collect();
        assert_eq!(spans, vec![("alpha", 0, 5), ("gamma", 12, 17)]);
    }

    #[test]
    fn an_appended_line_keeps_every_fact() {
        let old = "i32 Main() { return 0; }\n";
        let new = format!("{old}// edit\n");
        let doc = document(old, vec![hover(4, 8, "/w/Main.bd")], Vec::new());
        let carried = doc.carried_over(new, Some(std::path::Path::new("/w/Main.bd")));
        assert_eq!(carried.syntax_hovers.len(), 1);
        assert_eq!((carried.syntax_hovers[0].reference_start, carried.syntax_hovers[0].reference_end), (4, 8));
    }

    #[test]
    fn multibyte_text_splits_on_character_boundaries() {
        let doc = document("aé b", vec![hover(4, 5, "/w/x.bd")], Vec::new());
        let carried = doc.carried_over("aè b".to_string(), None);
        assert_eq!(carried.syntax_hovers.len(), 1, "the hover after the edit survives");
        assert_eq!(carried.syntax_hovers[0].reference_start, 4);
    }
}
