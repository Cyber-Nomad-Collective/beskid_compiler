//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::{SourceUnit, UnitHir};
use beskid_analysis::projects::CompilePlan;
use salsa::Setter;

use crate::inputs::{FileText, GrammarRevision, ProjectSession};
use crate::stats::record_revision_bump;

type ProjectRegistry = HashMap<(PathBuf, PathBuf, String), ProjectSession>;

/// Cached heavy artifacts keyed by content fingerprint (invalidated via Salsa inputs).
#[derive(Default)]
pub struct UnitArtifactCache {
    pub source_units: HashMap<String, Arc<SourceUnit>>,
    pub unit_hir: HashMap<String, Arc<UnitHir>>,
}

/// Salsa database trait extended by tracked query groups.
#[salsa::db]
pub trait Db: salsa::Database {
    fn file_registry(&self) -> &Mutex<HashMap<PathBuf, FileText>>;
    fn project_registry(&self) -> &Mutex<ProjectRegistry>;
    fn unit_cache(&self) -> &Mutex<UnitArtifactCache>;
    fn grammar_revision_input(&self) -> GrammarRevision;
}

/// Process/workspace-scoped incremental compilation database.
#[salsa::db]
#[derive(Clone)]
pub struct BeskidDatabase {
    storage: salsa::Storage<Self>,
    file_registry: Arc<Mutex<HashMap<PathBuf, FileText>>>,
    project_registry: Arc<Mutex<ProjectRegistry>>,
    unit_cache: Arc<Mutex<UnitArtifactCache>>,
    persistence_root: Option<PathBuf>,
    grammar_revision: Option<GrammarRevision>,
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
            unit_cache: Arc::new(Mutex::new(UnitArtifactCache::default())),
            persistence_root: persistence_root.clone(),
            grammar_revision: None,
        };
        let _ = db.grammar_revision();
        if let Some(root) = &db.persistence_root {
            let _ = crate::persistence::ensure_salsa_dir(root);
        }
        db
    }

    pub fn with_persistence(project_root: &Path) -> Self {
        Self::new(Some(
            project_root
                .join("obj")
                .join("beskid")
                .join("cache")
                .join("salsa"),
        ))
    }

    pub fn persistence_root(&self) -> Option<&Path> {
        self.persistence_root.as_deref()
    }

    /// Workspace grammar revision input; bumps invalidate all unit tracked queries.
    pub fn grammar_revision(&mut self) -> GrammarRevision {
        if let Some(rev) = self.grammar_revision {
            return rev;
        }
        let rev = GrammarRevision::new(self, beskid_pipeline::GRAMMAR_REVISION.to_string());
        self.grammar_revision = Some(rev);
        rev
    }

    pub fn grammar_revision_ref(&self) -> GrammarRevision {
        self.grammar_revision
            .expect("grammar revision initialized in BeskidDatabase::new")
    }

    /// Invalidate units that import `changed_path` (fine-grained edit propagation).
    pub fn invalidate_import_dependents(
        &mut self,
        session: ProjectSession,
        changed_path: PathBuf,
        candidate_paths: Vec<PathBuf>,
    ) {
        let db_ref: &dyn Db = self;
        let dependents = crate::graph::reverse_dependents(
            db_ref,
            session,
            changed_path.clone(),
            candidate_paths,
        );
        let mut fingerprints = Vec::new();
        for path in dependents {
            if let Some(file) = self.file_text(&path) {
                fingerprints.push(beskid_artifacts::content_fingerprint(file.text(self)));
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                fingerprints.push(beskid_artifacts::content_fingerprint(&text));
            }
        }
        self.invalidate_unit_fingerprints(&fingerprints);
    }

    pub fn clear_unit_cache(&self) {
        let mut cache = self.unit_cache.lock().expect("unit cache");
        cache.source_units.clear();
        cache.unit_hir.clear();
    }

    fn invalidate_unit_fingerprints(&self, fingerprints: &[String]) {
        if fingerprints.is_empty() {
            return;
        }
        let mut cache = self.unit_cache.lock().expect("unit cache");
        for fp in fingerprints {
            cache.source_units.remove(fp);
            cache.unit_hir.remove(fp);
        }
    }

    /// Register or update file text (canonical path key).
    pub fn set_file_text(&mut self, path: PathBuf, text: String) {
        let canonical = path.canonicalize().unwrap_or(path);
        let old_text = self.file_text(&canonical).map(|f| f.text(self).clone());
        record_revision_bump();
        let mut fps = vec![beskid_artifacts::content_fingerprint(&text)];
        if let Some(old) = old_text.as_deref() {
            fps.push(beskid_artifacts::content_fingerprint(old));
        }
        self.invalidate_unit_fingerprints(&fps);
        self.set_file_text_inner(canonical.clone(), text);
        if let Some(session) = self.active_project_session() {
            self.invalidate_import_dependents(session, canonical, self.known_file_paths());
        }
    }

    /// Register file text when changed; skips revision bump and cache clear on identical content.
    pub fn ensure_file_text(&mut self, path: PathBuf, text: String) {
        let canonical = path.canonicalize().unwrap_or(path.clone());
        if let Some(existing) = self.file_text(&canonical)
            && existing.text(self) == &text
        {
            return;
        }
        record_revision_bump();
        let old_text = self.file_text(&canonical).map(|f| f.text(self).clone());
        let mut fps = vec![beskid_artifacts::content_fingerprint(&text)];
        if let Some(old) = old_text.as_deref() {
            fps.push(beskid_artifacts::content_fingerprint(old));
        }
        self.invalidate_unit_fingerprints(&fps);
        self.set_file_text_inner(canonical.clone(), text);
        if let Some(session) = self.active_project_session() {
            self.invalidate_import_dependents(session, canonical, self.known_file_paths());
        }
    }

    fn known_file_paths(&self) -> Vec<PathBuf> {
        self.file_registry
            .lock()
            .expect("file registry")
            .keys()
            .cloned()
            .collect()
    }

    fn active_project_session(&self) -> Option<ProjectSession> {
        self.project_registry
            .lock()
            .expect("project registry")
            .values()
            .next()
            .copied()
    }

    fn set_file_text_inner(&mut self, canonical: PathBuf, text: String) {
        let existing = {
            let registry = self.file_registry.lock().expect("file registry");
            registry.get(&canonical).copied()
        };
        if let Some(existing) = existing {
            existing.set_text(self).to(text.clone());
        } else {
            let file = FileText::new(self, canonical.clone(), text.clone());
            self.file_registry
                .lock()
                .expect("file registry")
                .insert(canonical.clone(), file);
        }
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
        let existing = {
            let registry = self.project_registry.lock().expect("project registry");
            registry.get(&key).copied()
        };
        if let Some(existing) = existing {
            existing.set_lockfile_digest(self).to(lockfile_digest);
            return existing;
        }
        let session = ProjectSession::new(
            self,
            plan.project_root.clone(),
            key.1.clone(),
            plan.target.name.clone(),
            lockfile_digest,
        );
        self.project_registry
            .lock()
            .expect("project registry")
            .insert(key, session);
        session
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

    fn unit_cache(&self) -> &Mutex<UnitArtifactCache> {
        &self.unit_cache
    }

    fn grammar_revision_input(&self) -> GrammarRevision {
        self.grammar_revision_ref()
    }
}
