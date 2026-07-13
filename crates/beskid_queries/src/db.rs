//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::CompilePlan;
use beskid_analysis::projects::assembly::{ModuleIndex, SourceUnit, UnitHir};
use beskid_analysis::resolve::Resolution;
use beskid_analysis::types::UnitTypeSurface;
use salsa::Setter;

use crate::inputs::{FileText, GrammarRevision, ProjectSession};
use crate::semantic_contract::{SourceUnitId, SyntaxUnitInput};
use crate::stats::record_revision_bump;
use crate::typed_entry_bundle::reset_typed_entry_inputs;

type ProjectRegistry = HashMap<(PathBuf, PathBuf, String), ProjectSession>;
type ModuleIndexCache = HashMap<String, Arc<ModuleIndex>>;
type SyntaxUnitRegistry = HashMap<SourceUnitId, SyntaxUnitInput>;

/// Cached heavy artifacts keyed by content fingerprint (invalidated via Salsa inputs).
#[derive(Default)]
pub struct UnitArtifactCache {
    pub source_units: HashMap<String, Arc<SourceUnit>>,
    pub unit_hir: HashMap<String, Arc<UnitHir>>,
    pub unit_resolutions: HashMap<String, Arc<Resolution>>,
    pub unit_type_surfaces: HashMap<String, Arc<UnitTypeSurface>>,
}

/// Salsa database trait extended by tracked query groups.
#[salsa::db]
pub trait Db: salsa::Database {
    fn file_registry(&self) -> &Mutex<HashMap<PathBuf, FileText>>;
    fn project_registry(&self) -> &Mutex<ProjectRegistry>;
    fn unit_cache(&self) -> &Mutex<UnitArtifactCache>;
    fn grammar_revision_input(&self) -> GrammarRevision;
    fn module_index_cached(&self, fingerprint: &str) -> Option<Arc<ModuleIndex>>;
    fn syntax_unit(&self, unit: SourceUnitId) -> Option<SyntaxUnitInput>;
}

/// Process/workspace-scoped incremental compilation database.
#[salsa::db]
#[derive(Clone)]
pub struct BeskidDatabase {
    storage: salsa::Storage<Self>,
    file_registry: Arc<Mutex<HashMap<PathBuf, FileText>>>,
    project_registry: Arc<Mutex<ProjectRegistry>>,
    syntax_unit_registry: Arc<Mutex<SyntaxUnitRegistry>>,
    unit_cache: Arc<Mutex<UnitArtifactCache>>,
    module_index_cache: Arc<Mutex<ModuleIndexCache>>,
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
            syntax_unit_registry: Arc::new(Mutex::new(HashMap::new())),
            unit_cache: Arc::new(Mutex::new(UnitArtifactCache::default())),
            module_index_cache: Arc::new(Mutex::new(ModuleIndexCache::new())),
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

    /// Return the single registered Salsa revision input for `unit`.
    pub fn syntax_unit(&self, unit: SourceUnitId) -> Option<SyntaxUnitInput> {
        Db::syntax_unit(self, unit)
    }

    /// Return the registered input for `unit`, creating it when first observed.
    pub fn ensure_syntax_unit(
        &mut self,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
    ) -> SyntaxUnitInput {
        let registry = Arc::clone(&self.syntax_unit_registry);
        let mut registry = registry.lock().expect("syntax unit registry");
        if let Some(input) = registry.get(&unit).copied() {
            if input.generation(self) != generation {
                input.set_generation(self).to(generation);
            }
            return input;
        }
        let input = SyntaxUnitInput::new(self, unit, generation);
        registry.insert(unit, input);
        input
    }

    /// Update the generation of the existing registered input without replacing its Salsa id.
    pub fn update_syntax_unit(
        &mut self,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
    ) -> Option<SyntaxUnitInput> {
        let input = self.syntax_unit(unit)?;
        input.set_generation(self).to(generation);
        Some(input)
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
        cache.unit_resolutions.clear();
        cache.unit_type_surfaces.clear();
    }

    /// Register a module index snapshot for per-unit resolution queries.
    pub fn cache_module_index(&self, fingerprint: String, index: Arc<ModuleIndex>) {
        self.module_index_cache
            .lock()
            .expect("module index cache")
            .insert(fingerprint, index);
    }

    pub fn module_index_cached(&self, fingerprint: &str) -> Option<Arc<ModuleIndex>> {
        self.module_index_cache
            .lock()
            .expect("module index cache")
            .get(fingerprint)
            .cloned()
    }

    fn invalidate_unit_fingerprints(&self, fingerprints: &[String]) {
        if fingerprints.is_empty() {
            return;
        }
        let mut cache = self.unit_cache.lock().expect("unit cache");
        for fp in fingerprints {
            cache.source_units.remove(fp);
            cache.unit_hir.remove(fp);
            cache.unit_resolutions.remove(fp);
            cache.unit_type_surfaces.remove(fp);
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

/// Replace Salsa storage after clearing typed-entry input registries tied to the old storage.
pub fn replace_compilation_database(target: &mut BeskidDatabase, replacement: BeskidDatabase) {
    reset_typed_entry_inputs();
    *target = replacement;
}

/// Reset to an in-memory database (no on-disk persistence).
pub fn reset_compilation_database(target: &mut BeskidDatabase) {
    replace_compilation_database(target, BeskidDatabase::default());
}

/// Reconfigure persistence for `project_root`, replacing storage when the root changes.
pub fn configure_compilation_database_for_project(
    target: &mut BeskidDatabase,
    project_root: &Path,
) {
    replace_compilation_database(target, BeskidDatabase::with_persistence(project_root));
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

    fn syntax_unit(&self, unit: SourceUnitId) -> Option<SyntaxUnitInput> {
        self.syntax_unit_registry
            .lock()
            .expect("syntax unit registry")
            .get(&unit)
            .copied()
    }

    fn unit_cache(&self) -> &Mutex<UnitArtifactCache> {
        &self.unit_cache
    }

    fn grammar_revision_input(&self) -> GrammarRevision {
        self.grammar_revision_ref()
    }

    fn module_index_cached(&self, fingerprint: &str) -> Option<Arc<ModuleIndex>> {
        self.module_index_cached(fingerprint)
    }
}
