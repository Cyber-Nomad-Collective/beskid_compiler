//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::{SourceUnit, UnitHir};
use beskid_analysis::projects::CompilePlan;
use salsa::Setter;

use crate::inputs::{FileText, ProjectSession};
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
}

impl Default for BeskidDatabase {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BeskidDatabase {
    pub fn new(persistence_root: Option<PathBuf>) -> Self {
        let db = Self {
            storage: salsa::Storage::default(),
            file_registry: Arc::new(Mutex::new(HashMap::new())),
            project_registry: Arc::new(Mutex::new(HashMap::new())),
            unit_cache: Arc::new(Mutex::new(UnitArtifactCache::default())),
            persistence_root: persistence_root.clone(),
        };
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

    pub fn clear_unit_cache(&self) {
        let mut cache = self.unit_cache.lock().expect("unit cache");
        cache.source_units.clear();
        cache.unit_hir.clear();
    }

    /// Register or update file text (canonical path key).
    pub fn set_file_text(&mut self, path: PathBuf, text: String) {
        record_revision_bump();
        self.clear_unit_cache();
        self.set_file_text_inner(path, text);
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
        self.clear_unit_cache();
        self.set_file_text_inner(canonical, text);
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
}
