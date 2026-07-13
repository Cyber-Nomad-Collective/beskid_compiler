//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::CompilePlan;
use beskid_analysis::projects::assembly::{ModuleIndex, SourceUnit, UnitHir};
use beskid_analysis::resolve::Resolution;
use beskid_analysis::types::UnitTypeSurface;
use salsa::Setter;

use crate::inputs::{FileText, GrammarRevision, ProjectSession};
use crate::semantic_contract::{SemanticError, SourceUnitId, SyntaxUnitInput, SyntaxUnitRevision};
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
    syntax_parse_count: Arc<AtomicU64>,
    syntax_index_build_count: Arc<AtomicU64>,
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
            syntax_parse_count: Arc::new(AtomicU64::new(0)),
            syntax_index_build_count: Arc::new(AtomicU64::new(0)),
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
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let source = self
            .file_text(unit.path(self))
            .map(|file| file.text(self).clone())
            .ok_or_else(|| {
                SemanticError::new(format!(
                    "source text is not registered for {}",
                    unit.path(self).display()
                ))
            })?;
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(&source));
        if let Some(input) = self.syntax_unit(unit) {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        self.register_expanded_syntax(
            project,
            unit,
            generation,
            source_fingerprint,
            Arc::new(program),
        )
    }

    /// Parse, expand, register, and invalidate one edited source as a single semantic update.
    pub fn update_syntax_source(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source: String,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(&source));
        if let Some(input) = self.syntax_unit(unit) {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        let input = self.register_expanded_syntax(
            project,
            unit,
            generation,
            source_fingerprint,
            Arc::new(program),
        )?;
        self.ensure_file_text(unit.path(self).clone(), source);
        Ok(input)
    }

    fn register_expanded_syntax(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source_fingerprint: Arc<str>,
        expanded_program: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&expanded_program)?);
        let registry = Arc::clone(&self.syntax_unit_registry);
        let mut registry = registry.lock().expect("syntax unit registry");
        if let Some(input) = registry.get(&unit).copied() {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            let current = input.revision(self);
            if current
                .tree_fingerprint_history
                .iter()
                .any(|fingerprint| fingerprint == &tree_fingerprint)
            {
                return Err(SemanticError::new(
                    "expanded syntax cannot reuse a tree fingerprint from an existing generation",
                ));
            }
            let syntax_index = Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(
                &expanded_program,
                generation,
            ));
            self.syntax_index_build_count
                .fetch_add(1, Ordering::Relaxed);
            let mut source_fingerprint_history = current.source_fingerprint_history.to_vec();
            source_fingerprint_history.push(Arc::clone(&source_fingerprint));
            let mut tree_fingerprint_history = current.tree_fingerprint_history.to_vec();
            tree_fingerprint_history.push(Arc::clone(&tree_fingerprint));
            input.set_revision(self).to(Arc::new(SyntaxUnitRevision {
                generation,
                expanded_program,
                syntax_index,
                source_fingerprint,
                tree_fingerprint,
                source_fingerprint_history: source_fingerprint_history.into(),
                tree_fingerprint_history: tree_fingerprint_history.into(),
            }));
            return Ok(input);
        }
        let syntax_index = Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(
            &expanded_program,
            generation,
        ));
        self.syntax_index_build_count
            .fetch_add(1, Ordering::Relaxed);
        let input = SyntaxUnitInput::new(
            self,
            project,
            unit,
            Arc::new(SyntaxUnitRevision {
                generation,
                expanded_program,
                syntax_index,
                source_fingerprint_history: Arc::from([Arc::clone(&source_fingerprint)]),
                tree_fingerprint_history: Arc::from([Arc::clone(&tree_fingerprint)]),
                source_fingerprint,
                tree_fingerprint,
            }),
        );
        registry.insert(unit, input);
        Ok(input)
    }

    fn validate_existing_registration(
        &self,
        input: SyntaxUnitInput,
        project: ProjectSession,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source_fingerprint: &Arc<str>,
    ) -> Result<(), SemanticError> {
        if input.project(self) != project {
            return Err(SemanticError::new(
                "a source unit cannot be reassigned to another project session",
            ));
        }
        let current_generation = input.generation(self);
        let source_changed = input.source_fingerprint(self) != source_fingerprint;
        if generation.0 < current_generation.0 {
            return Err(SemanticError::new("syntax generation cannot regress"));
        }
        if source_changed && generation.0 == current_generation.0 {
            return Err(SemanticError::new(
                "changed syntax requires a strictly newer generation",
            ));
        }
        if source_changed
            && input
                .revision(self)
                .source_fingerprint_history
                .iter()
                .any(|fingerprint| fingerprint == source_fingerprint)
        {
            return Err(SemanticError::new(
                "source syntax cannot resurrect a fingerprint from an earlier generation",
            ));
        }
        if !source_changed && generation != current_generation {
            return Err(SemanticError::new(
                "unchanged syntax cannot be relabeled with a different generation",
            ));
        }
        Ok(())
    }

    fn parse_and_expand(
        &self,
        unit: SourceUnitId,
        source: &str,
    ) -> Result<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>, SemanticError>
    {
        self.syntax_parse_count.fetch_add(1, Ordering::Relaxed);
        let source_name = unit.path(self).display().to_string();
        let program =
            beskid_analysis::services::parse_program_with_source_name(&source_name, source)
                .map_err(|error| {
                    SemanticError::new(format!("failed to parse {source_name}: {error}"))
                })?;
        let expanded = beskid_analysis::macros::expand_program_with_diagnostics(
            program,
            beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            &source_name,
            source,
        );
        let errors = expanded
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == beskid_analysis::analysis::Severity::Error)
            .map(|diagnostic| format!("failed to expand {source_name}: {}", diagnostic.message))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(SemanticError::from_diagnostics(errors));
        }
        Ok(expanded.program)
    }

    #[doc(hidden)]
    pub fn syntax_authority_counts(&self) -> (u64, u64) {
        (
            self.syntax_parse_count.load(Ordering::Relaxed),
            self.syntax_index_build_count.load(Ordering::Relaxed),
        )
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

fn expanded_syntax_fingerprint(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Result<String, SemanticError> {
    let mut structural = serde_json::to_value(program).map_err(|error| {
        SemanticError::new(format!("failed to fingerprint expanded syntax: {error}"))
    })?;
    remove_span_fields(&mut structural);
    let encoded = serde_json::to_string(&structural).map_err(|error| {
        SemanticError::new(format!("failed to fingerprint expanded syntax: {error}"))
    })?;
    Ok(beskid_artifacts::content_fingerprint(&encoded))
}

fn remove_span_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                remove_span_fields(value);
            }
        }
        serde_json::Value::Object(fields) => {
            fields.remove("span");
            for value in fields.values_mut() {
                remove_span_fields(value);
            }
        }
        _ => {}
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
