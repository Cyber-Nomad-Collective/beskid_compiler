use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::assembly::ModuleIndex;

use crate::inputs::ProjectSession;

use super::{BeskidDatabase, Db};

impl BeskidDatabase {
    /// Invalidate units that import `changed_path` (fine-grained edit propagation).
    pub fn invalidate_import_dependents(
        &mut self,
        session: ProjectSession,
        changed_path: PathBuf,
        candidate_paths: Vec<PathBuf>,
    ) {
        let db_ref: &dyn Db = self;
        let dependents = crate::graph::reverse_dependents(db_ref, session, changed_path.clone(), candidate_paths);
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
    }

    /// Register a module index snapshot for per-unit resolution queries.
    pub fn cache_module_index(&self, fingerprint: String, index: Arc<ModuleIndex>) {
        self.module_index_cache.lock().expect("module index cache").insert(fingerprint, index);
    }

    pub fn module_index_cached(&self, fingerprint: &str) -> Option<Arc<ModuleIndex>> {
        self.module_index_cache.lock().expect("module index cache").get(fingerprint).cloned()
    }

    pub(super) fn invalidate_unit_fingerprints(&self, fingerprints: &[String]) {
        if fingerprints.is_empty() {
            return;
        }
        let mut cache = self.unit_cache.lock().expect("unit cache");
        for fp in fingerprints {
            cache.source_units.remove(fp);
        }
    }
}
