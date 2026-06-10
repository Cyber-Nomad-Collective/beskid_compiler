use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use beskid_analysis::CompilationContext;
use beskid_analysis::services::DocumentAnalysisSnapshot;
use beskid_queries::BeskidDatabase;
use tokio::sync::Mutex as AsyncMutex;
use tower_lsp_server::ls_types::Uri;

use super::db_access;

/// One editor buffer or disk snapshot with optional precomputed analysis.
#[derive(Debug, Clone)]
pub struct Document {
    pub version: i32,
    pub text: String,
    pub analysis_cache_version: u32,
    pub analysis: Option<DocumentAnalysisSnapshot>,
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

    pub fn configure_db_for_project(&mut self, project_root: &std::path::Path) {
        let canonical = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        if self.configured_project_root.as_ref() == Some(&canonical) {
            return;
        }
        {
            let mut db = self.compilation_db.lock().expect("compilation db lock");
            *db = BeskidDatabase::with_persistence(&canonical);
        }
        self.configured_project_root = Some(canonical);
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
        *db = BeskidDatabase::with_persistence(&canonical);
        self.configured_project_root = Some(canonical);
    }

    pub fn reset_compilation_db(&mut self) {
        {
            let mut db = self.compilation_db.lock().expect("compilation db lock");
            *db = BeskidDatabase::default();
        }
        self.configured_project_root = None;
    }

    pub fn reset_compilation_db_with_db(&mut self, db: &mut BeskidDatabase) {
        *db = BeskidDatabase::default();
        self.configured_project_root = None;
    }
}
