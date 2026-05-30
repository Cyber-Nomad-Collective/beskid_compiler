//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::CompilePlan;
use salsa::Setter;

use crate::inputs::{FileText, GrammarRevision, ProjectSession};
use crate::stats::record_revision_bump;

/// Salsa database trait extended by tracked query groups.
#[salsa::db]
pub trait Db: salsa::Database {
    fn file_registry(&self) -> &Mutex<HashMap<PathBuf, FileText>>;
    fn project_registry(&self) -> &Mutex<HashMap<(PathBuf, PathBuf, String), ProjectSession>>;
    fn grammar_revision(&self) -> GrammarRevision;
}

/// Process/workspace-scoped incremental compilation database.
#[salsa::db]
#[derive(Clone)]
pub struct BeskidDatabase {
    storage: salsa::Storage<Self>,
    file_registry: Arc<Mutex<HashMap<PathBuf, FileText>>>,
    project_registry: Arc<Mutex<HashMap<(PathBuf, PathBuf, String), ProjectSession>>>,
    grammar_revision: GrammarRevision,
    persistence_root: Option<PathBuf>,
}

impl Default for BeskidDatabase {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BeskidDatabase {
    pub fn new(persistence_root: Option<PathBuf>) -> Self {
        let mut db = Self {
            storage: salsa::Storage::default(),
            file_registry: Arc::new(Mutex::new(HashMap::new())),
            project_registry: Arc::new(Mutex::new(HashMap::new())),
            grammar_revision: GrammarRevision::new(
                &salsa::Storage::<Self>::default(),
                String::new(),
            ),
            persistence_root: persistence_root.clone(),
        };
        db.grammar_revision =
            GrammarRevision::new(&db, crate::inputs::default_grammar_revision());
        if let Some(root) = &db.persistence_root {
            let _ = crate::persistence::ensure_salsa_dir(root);
        }
        db
    }

    pub fn with_persistence(project_root: &Path) -> Self {
        let cache = project_root
            .join("obj")
            .join("beskid")
            .join("cache")
            .join("salsa");
        Self::new(Some(cache))
    }

    pub fn persistence_root(&self) -> Option<&Path> {
        self.persistence_root.as_deref()
    }

    /// Register or update file text (canonical path key).
    pub fn set_file_text(&mut self, path: PathBuf, text: String) {
        record_revision_bump();
        let canonical = path.canonicalize().unwrap_or(path.clone());
        let mut registry = self.file_registry.lock().expect("file registry");
        if let Some(existing) = registry.get(&canonical) {
            existing.set_text(self).to(text);
            if let Some(root) = &self.persistence_root {
                let _ = crate::persistence::persist_file_text(root, &canonical, existing.text(self));
            }
            return;
        }
        let file = FileText::new(self, canonical.clone(), text.clone());
        registry.insert(canonical.clone(), file);
        if let Some(root) = &self.persistence_root {
            let _ = crate::persistence::persist_file_text(root, &canonical, &text);
        }
    }

    pub fn file_text(&self, path: &Path) -> Option<FileText> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.file_registry
            .lock()
            .expect("file registry")
            .get(&canonical)
            .copied()
    }

    pub fn ensure_project_session(
        &mut self,
        plan: &CompilePlan,
        entry_path: &Path,
        lockfile_digest: String,
    ) -> ProjectSession {
        let key = (
            plan.project_root.clone(),
            entry_path
                .canonicalize()
                .unwrap_or_else(|_| entry_path.to_path_buf()),
            plan.target.name.clone(),
        );
        let mut registry = self.project_registry.lock().expect("project registry");
        if let Some(existing) = registry.get(&key) {
            existing.set_lockfile_digest(self).to(lockfile_digest);
            return *existing;
        }
        let session = ProjectSession::new(
            self,
            plan.project_root.clone(),
            key.1.clone(),
            plan.target.name.clone(),
            lockfile_digest,
        );
        registry.insert(key, session);
        session
    }

    pub fn grammar_revision_input(&self) -> GrammarRevision {
        self.grammar_revision
    }
}

#[salsa::db]
impl salsa::Database for BeskidDatabase {}

#[salsa::db]
impl Db for BeskidDatabase {
    fn file_registry(&self) -> &Mutex<HashMap<PathBuf, FileText>> {
        &self.file_registry
    }

    fn project_registry(&self) -> &Mutex<HashMap<(PathBuf, PathBuf, String), ProjectSession>> {
        &self.project_registry
    }

    fn grammar_revision(&self) -> GrammarRevision {
        self.grammar_revision
    }
}
