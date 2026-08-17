//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

mod caches;
mod files;
mod lifecycle;
mod sessions;
mod syntax;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::{ModuleIndex, SourceUnit};
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::inputs::{FileText, GrammarRevision, ProjectSession};
use crate::semantic_contract::{CorelibService, SourceUnitId, SyntaxUnitInput};
use crate::typed_entry_bundle::reset_typed_entry_inputs;

type ProjectRegistry = HashMap<(PathBuf, PathBuf, String), ProjectSession>;
type ModuleIndexCache = HashMap<String, Arc<ModuleIndex>>;
type SyntaxUnitRegistry = HashMap<SourceUnitId, SyntaxUnitInput>;

/// Assembly-scoped import/module authority for generation-safe cross-unit syntax facts.
#[derive(Default)]
pub struct SyntaxDependencyRegistry {
    pub(crate) imports: HashMap<(SourceUnitId, SyntaxGenerationId), Vec<SyntaxImport>>,
    /// Exact logical module paths assembled for one syntax generation.
    pub(crate) modules: HashMap<(SyntaxGenerationId, Vec<String>), Vec<SourceUnitId>>,
    /// Compiler-minted Corelib service names available to one exact source unit generation.
    /// Ordinary program syntax never populates this registry entry.
    pub(crate) corelib_services: HashMap<(SourceUnitId, SyntaxGenerationId), Vec<CorelibService>>,
}

/// One explicit module import resolved to an assembled syntax unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxImport {
    pub(crate) path: Vec<String>,
    pub(crate) binding: String,
    /// Whether the source declaration explicitly named this binding with `as`.
    pub(crate) has_explicit_alias: bool,
    pub(crate) target: SourceUnitId,
    /// Whether the importing source unit exposes this target through its module API.
    /// Private imports remain available only to the importing unit's own syntax facts.
    pub(crate) public: bool,
}

/// Cached heavy artifacts keyed by content fingerprint (invalidated via Salsa inputs).
#[derive(Default)]
pub struct UnitArtifactCache {
    pub source_units: HashMap<String, Arc<SourceUnit>>,
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
    fn syntax_dependency_registry(&self) -> &Mutex<SyntaxDependencyRegistry>;
}

/// Process/workspace-scoped incremental compilation database.
#[salsa::db]
#[derive(Clone)]
pub struct BeskidDatabase {
    storage: salsa::Storage<Self>,
    file_registry: Arc<Mutex<HashMap<PathBuf, FileText>>>,
    project_registry: Arc<Mutex<ProjectRegistry>>,
    syntax_unit_registry: Arc<Mutex<SyntaxUnitRegistry>>,
    syntax_dependency_registry: Arc<Mutex<SyntaxDependencyRegistry>>,
    unit_cache: Arc<Mutex<UnitArtifactCache>>,
    module_index_cache: Arc<Mutex<ModuleIndexCache>>,
    persistence_root: Option<PathBuf>,
    grammar_revision: Option<GrammarRevision>,
    syntax_parse_count: Arc<AtomicU64>,
    syntax_index_build_count: Arc<AtomicU64>,
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
pub fn configure_compilation_database_for_project(target: &mut BeskidDatabase, project_root: &Path) {
    replace_compilation_database(target, BeskidDatabase::with_persistence(project_root));
    // Cross-run salsa snapshot is now loaded inside `BeskidDatabase::new` (before
    // GrammarRevision allocation) to avoid salsa's allocation-order assertion. See
    // `lifecycle.rs` for the rationale.
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
        self.syntax_unit_registry.lock().expect("syntax unit registry").get(&unit).copied()
    }

    fn syntax_dependency_registry(&self) -> &Mutex<SyntaxDependencyRegistry> {
        &self.syntax_dependency_registry
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
