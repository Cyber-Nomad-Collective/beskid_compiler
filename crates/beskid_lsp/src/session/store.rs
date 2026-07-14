use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use beskid_analysis::CompilationContext;
use beskid_analysis::services::DocumentAnalysisSnapshot;
use beskid_queries::{
    BeskidDatabase, configure_compilation_database_for_project, reset_compilation_database,
};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tower_lsp_server::ls_types::Uri;

use super::db_access;

/// One editor buffer or disk snapshot with optional precomputed analysis.
#[derive(Debug, Clone)]
pub struct Document {
    pub version: i32,
    pub text: String,
    pub analysis_cache_version: u32,
    pub analysis: Option<DocumentAnalysisSnapshot>,
    /// Generation-safe syntax/Salsa definition facts for this exact buffer revision.
    ///
    /// Definition handling consumes this index instead of reaching back into the legacy
    /// HIR-backed analysis snapshot.
    pub syntax_definitions: Vec<SyntaxDefinition>,
    /// Generation-safe syntax/Salsa hover facts for this exact buffer revision.
    pub syntax_hovers: Vec<SyntaxHover>,
    pub syntax_symbols: Vec<SyntaxSymbol>,
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
        }
    }
}

impl State {
    pub fn document_union(&self, uri: &Uri) -> Option<Document> {
        self.docs
            .get(uri)
            .cloned()
            .or_else(|| self.workspace_index.get(uri).cloned())
    }

    pub fn configure_db_for_project_with_db(
        &mut self,
        db: &mut BeskidDatabase,
        project_root: &std::path::Path,
    ) {
        let canonical = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
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
