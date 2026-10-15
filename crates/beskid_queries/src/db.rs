//! `BeskidDatabase`: Salsa storage host for CLI, LSP, and tests.

mod caches;
mod files;
mod lifecycle;
mod sessions;
mod syntax;
pub(crate) use syntax::expanded_syntax_fingerprint;

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
    pub(crate) package_identities:
        HashMap<(ProjectSession, SyntaxGenerationId), beskid_analysis::projects::VerifiedPackageIdentities>,
    pub(crate) imports: HashMap<(SourceUnitId, SyntaxGenerationId), Vec<SyntaxImport>>,
    /// Exact logical module paths assembled for one syntax generation.
    /// Written only through [`SyntaxDependencyRegistry::publish_modules`], which keeps the derived
    /// per-generation indexes below in step.
    pub(crate) modules: HashMap<(SyntaxGenerationId, Vec<String>), Vec<SourceUnitId>>,
    /// Every unit of any module of one generation, each once.
    generation_units: HashMap<SyntaxGenerationId, Vec<SourceUnitId>>,
    /// The least module path (in `Vec<String>` order) that contains one unit in one generation.
    least_unit_module: HashMap<(SourceUnitId, SyntaxGenerationId), Vec<String>>,
    /// Compiler-minted Corelib service names available to one exact source unit generation.
    /// Ordinary program syntax never populates this registry entry.
    pub(crate) corelib_services: HashMap<(SourceUnitId, SyntaxGenerationId), Vec<CorelibService>>,
    /// Canonical logical path proven by embedded bytes and trusted physical origin.
    pub(crate) corelib_source_paths: HashMap<(SourceUnitId, SyntaxGenerationId), String>,
    /// Per entry unit and generation: the top-level items appended by the Mod merger that a
    /// retained serialization claim attributes to another registered unit (the unit that
    /// declares the claim's target). Derived from the issuer payload at registration; see
    /// `contribution_attribution`.
    pub(crate) contribution_attributions:
        HashMap<(SourceUnitId, SyntaxGenerationId), std::sync::Arc<[(crate::AstNodeId, SourceUnitId)]>>,
}

impl SyntaxDependencyRegistry {
    /// Resolve an assembled module path. Every package keeps its native logical paths
    /// (`Core.*`, `Testing.*`, host modules unprefixed), so all source units share one namespace.
    pub(crate) fn visible_module_units(
        &self,
        generation: SyntaxGenerationId,
        path: &[String],
    ) -> Option<&[SourceUnitId]> {
        self.modules.get(&(generation, path.to_vec())).map(Vec::as_slice)
    }

    /// Register the units of logical modules assembled for `generation` and rebuild that
    /// generation's derived indexes.
    pub(crate) fn publish_modules(
        &mut self,
        generation: SyntaxGenerationId,
        modules: impl IntoIterator<Item = (Vec<String>, Vec<SourceUnitId>)>,
    ) {
        for (path, units) in modules {
            self.modules.insert((generation, path), units);
        }
        let mut units = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.least_unit_module.retain(|(_, unit_generation), _| *unit_generation != generation);
        for ((module_generation, path), module_units) in &self.modules {
            if *module_generation != generation {
                continue;
            }
            for unit in module_units {
                if seen.insert(*unit) {
                    units.push(*unit);
                }
                let least = self.least_unit_module.entry((*unit, generation)).or_insert_with(|| path.clone());
                if path < least {
                    *least = path.clone();
                }
            }
        }
        self.generation_units.insert(generation, units);
    }

    /// Every unit of any module assembled for `generation`.
    pub(crate) fn generation_units(&self, generation: SyntaxGenerationId) -> &[SourceUnitId] {
        self.generation_units.get(&generation).map_or(&[], Vec::as_slice)
    }

    /// The least logical module path that contains `unit` in `generation`.
    pub(crate) fn least_unit_module(&self, unit: SourceUnitId, generation: SyntaxGenerationId) -> Option<&[String]> {
        self.least_unit_module.get(&(unit, generation)).map(Vec::as_slice)
    }
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
    /// The declaration that created this edge.
    pub(crate) origin: SyntaxImportOrigin,
}

/// The source construct that created one [`SyntaxImport`] edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxImportOrigin {
    /// A `use Path;` or `use Path as Alias;` declaration. Its binding is a name in the owner's
    /// scope, so it shadows a package-native module root of the same spelling in qualified paths.
    Use,
    /// An out-of-line `mod A.B;` declaration: a namespace edge to the declared child module. It
    /// does not bind a scope name that hides the package-native module namespace.
    ModuleDeclaration,
    /// The compiler-installed private scope of the verified canonical runtime corpus.
    CanonicalRuntimeScope,
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
    /// Another handle of the same storage and registries for a worker thread. Handles share
    /// every memo and input; a write through any handle waits until the others are dropped, so
    /// read-only parallel work (code emission) observes exactly one revision.
    fn fork_db(&self) -> Box<dyn Db>;
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

    fn fork_db(&self) -> Box<dyn Db> {
        Box::new(self.clone())
    }
}
