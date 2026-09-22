use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::inputs::GrammarRevision;

use super::{BeskidDatabase, ModuleIndexCache, SyntaxDependencyRegistry, UnitArtifactCache};

impl Default for BeskidDatabase {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BeskidDatabase {
    pub fn new(persistence_root: Option<PathBuf>) -> Self {
        let mut db = Self::uninitialized(persistence_root);
        // Snapshot loading owns a separate candidate and publishes it only after
        // successful deserialization. The fallback remains entirely pristine.
        if let Some(root) = db.persistence_root.clone() {
            let _ = crate::persistence::ensure_salsa_dir(&root);
            crate::persistence::load_db_snapshot(&mut db, &root);
        }
        let _ = db.grammar_revision();
        db
    }

    /// Empty storage for transactional snapshot loading, before input allocation.
    pub(crate) fn uninitialized(persistence_root: Option<PathBuf>) -> Self {
        Self {
            storage: salsa::Storage::default(),
            file_registry: Arc::new(Mutex::new(HashMap::new())),
            project_registry: Arc::new(Mutex::new(HashMap::new())),
            syntax_unit_registry: Arc::new(Mutex::new(HashMap::new())),
            syntax_dependency_registry: Arc::new(Mutex::new(SyntaxDependencyRegistry::default())),
            unit_cache: Arc::new(Mutex::new(UnitArtifactCache::default())),
            module_index_cache: Arc::new(Mutex::new(ModuleIndexCache::new())),
            persistence_root: persistence_root.clone(),
            grammar_revision: None,
            syntax_parse_count: Arc::new(AtomicU64::new(0)),
            syntax_index_build_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_persistence(project_root: &Path) -> Self {
        Self::new(Some(project_root.join("obj").join("beskid").join("cache").join("salsa")))
    }

    pub fn persistence_root(&self) -> Option<&Path> {
        self.persistence_root.as_deref()
    }

    /// Workspace grammar revision input; bumps invalidate all unit tracked queries.
    pub fn grammar_revision(&mut self) -> GrammarRevision {
        if let Some(rev) = self.grammar_revision {
            return rev;
        }
        // If the snapshot loaded a GrammarRevision, reuse it instead of allocating a duplicate.
        use salsa::plumbing::ZalsaDatabase;
        let loaded = GrammarRevision::ingredient(self).entries(self.zalsa()).next().map(|entry| entry.as_struct());
        if let Some(rev) = loaded {
            self.grammar_revision = Some(rev);
            return rev;
        }
        let rev = GrammarRevision::new(self, beskid_pipeline::GRAMMAR_REVISION.to_string());
        self.grammar_revision = Some(rev);
        rev
    }

    pub fn grammar_revision_ref(&self) -> GrammarRevision {
        self.grammar_revision.expect("grammar revision initialized in BeskidDatabase::new")
    }
}
