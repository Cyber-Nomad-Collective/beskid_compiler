use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use beskid_analysis::CompilationContext;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, configure_compilation_database_for_project, reset_compilation_database,
};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tower_lsp_server::ls_types::Uri;

use super::db_access;
use super::documentation_facts::SyntaxDocumentationFact;

// Re-export the single host-side `SyntaxFix` implementation so LSP code can name it as
// `crate::session::store::SyntaxFix` (DRY — no duplicate LSP-side type). Defined in
// `beskid_analysis::mod_host::diagnostics` and returned by the prepare spine.
pub use beskid_analysis::{SyntaxFix, SyntaxTextEdit, SyntaxTextEditKind};

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
        self.syntax_completion = None;
        self.syntax_inlay_hints.clear();
        self.syntax_documentation.clear();
        self.syntax_diagnostics.clear();
        self.syntax_fixes.clear();
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxCompletion {
    pub anchor: AstNodeKey,
}

/// In-memory LSP workspace: open docs, closed-but-indexed files, and compilation context cache.
pub struct State {
    /// Canonical `.bproj` path from init options or `beskid.focusedProjectUri` configuration.
    pub focused_project: Option<PathBuf>,
    pub docs: HashMap<Uri, Document>,
    /// Closed files on disk that still receive diagnostics (not managed by the editor buffer).
    pub workspace_index: HashMap<Uri, Document>,
    /// Key: canonical `.bproj` path plus `workspace_member_for_meta_default` (from graph
    /// build options / env), so `attachTo: default` disambiguation cannot reuse a stale slice.
    pub compilation_context_cache: HashMap<(PathBuf, Option<String>), CompilationContext>,
    /// Salsa incremental database shared by IDE features and diagnostics.
    pub compilation_db: Arc<Mutex<BeskidDatabase>>,
    /// Canonical project root the Salsa database was configured for (avoids wholesale resets).
    pub configured_project_root: Option<PathBuf>,
    /// Serializes all Salsa database operations across concurrent LSP handlers.
    pub(crate) db_gate: Arc<AsyncMutex<()>>,
    /// Coalesced typed-prepare schedule revision per open URI (debounced rebuild).
    pub typed_prepare_schedule_revision: HashMap<Uri, u64>,
    /// Set after the first workspace scan finishes (gates Salsa work during startup).
    pub(crate) initial_scan_complete: Arc<AtomicBool>,
    /// Wakes tasks waiting on [`initial_scan_complete`].
    pub(crate) scan_barrier: Arc<Notify>,
    /// Whether the LSP persists the Salsa DB snapshot to disk on idle/shutdown.
    ///
    /// Toggled via `persistenceEnabled` init option or `beskid.lsp.persistence.enabled`
    /// workspace config. When `false`, the LSP skips snapshot saves entirely (users
    /// who want only CLI saves). Defaults to `true`.
    pub persistence_save_enabled: bool,
    /// Idle debounce window before the LSP persists the Salsa DB snapshot.
    ///
    /// Configured via `persistenceSaveDebounceMs` init option or
    /// `beskid.lsp.persistence.saveDebounceMs` workspace config. Defaults to 5s.
    pub persistence_save_debounce: Duration,
    /// Coalesced snapshot-save schedule revision (debounced persistence).
    ///
    /// Single global counter (the DB is shared across all URIs) — each document
    /// change bumps it, and a spawned save only fires when it is still the latest.
    pub(crate) persistence_save_revision: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            focused_project: None,
            docs: HashMap::new(),
            workspace_index: HashMap::new(),
            compilation_context_cache: HashMap::new(),
            compilation_db: Arc::new(Mutex::new(BeskidDatabase::default())),
            configured_project_root: None,
            db_gate: db_access::new_db_gate(),
            typed_prepare_schedule_revision: HashMap::new(),
            initial_scan_complete: Arc::new(AtomicBool::new(false)),
            scan_barrier: Arc::new(Notify::new()),
            persistence_save_enabled: true,
            persistence_save_debounce: super::lifecycle::DEFAULT_PERSISTENCE_DEBOUNCE,
            persistence_save_revision: 0,
        }
    }
}

impl State {
    pub fn document_union(&self, uri: &Uri) -> Option<Document> {
        self.docs.get(uri).cloned().or_else(|| self.workspace_index.get(uri).cloned())
    }

    pub fn configure_db_for_project_with_db(&mut self, db: &mut BeskidDatabase, project_root: &std::path::Path) {
        let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
        if self.configured_project_root.as_ref() == Some(&canonical) {
            return;
        }
        configure_compilation_database_for_project(db, &canonical);
        self.configured_project_root = Some(canonical);
    }

    pub fn reset_compilation_db_with_db(&mut self, db: &mut BeskidDatabase) {
        reset_compilation_database(db);
        self.configured_project_root = None;
    }

    pub(crate) fn mark_initial_scan_complete(&self) {
        self.initial_scan_complete.store(true, Ordering::Release);
        self.scan_barrier.notify_waiters();
    }
}
