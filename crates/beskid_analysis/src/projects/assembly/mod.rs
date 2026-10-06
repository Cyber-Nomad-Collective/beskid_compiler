//! Syntax-only multi-module project assembly from [`CompilePlan`] and materialized source roots.

mod discovery;
mod loader;
mod module_index;
mod roots;
mod runtime_fixture;
mod unit_builder;
mod unit_cache;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub use discovery::{module_path_exists_on_disk, module_path_to_relative_path, resolve_module_file};
pub(crate) use loader::assemble_program;
pub use loader::{
    AssemblyError, UnitMaterializer, assemble_program_with_materializer, assembly_options_for_plan,
    assembly_options_for_prepare,
};
pub use module_index::{AssemblyModule, ModuleGraph, ModuleIndex, infer_logical_module_path};
pub use roots::{
    EffectiveCompilationRoots, RootEntry, effective_roots_for_plan, effective_roots_from_lockfile,
    effective_roots_from_lockfile_checked, effective_roots_from_plan_and_workspace, module_roots_from_effective,
};
pub use unit_builder::UnitBuilder;
pub use unit_cache::{
    UnitCacheStats, cache_root_for_project, disk_cache_stats, ensure_manifest, unit_content_fingerprint,
    unit_fingerprint,
};

use crate::{
    projects::AssemblyDiscovery,
    syntax::{Program, Spanned, SyntaxGenerationId},
    syntax_query::SyntaxIndex,
};

/// One parsed and macro-expanded compilation unit.
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub logical_name: String,
    /// Requested source location before resolving filesystem aliases; not a semantic key or authority grant.
    pub origin_path: PathBuf,
    /// Canonical semantic key, derived independently from the requested origin.
    pub path: PathBuf,
    pub source: String,
    pub program: Spanned<Program>,
}

impl SourceUnit {
    /// Bind reusable syntax to the current source request without inheriting cached path metadata.
    pub fn bind_request(origin_path: PathBuf, logical_name: String, source: String, program: Spanned<Program>) -> Self {
        Self { path: crate::paths::unit_path_key(&origin_path), origin_path, logical_name, source, program }
    }
}

/// Units an assembly judges as roots: type-checked, rule-checked, and reported as the
/// project's own code. Every other unit is a dependency (signatures plus best-effort bodies whose
/// errors are not reported against the consumer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssemblyRootSet {
    /// The entry unit (`ProgramAssembly::entry_index`) is the single root.
    Entry,
    /// Entry-less workspace scan of a library: every unit under the host project's own source root
    /// is a root. Holds the canonical unit path keys in assembly order; `entry_index` names the
    /// first of them and carries no "main entry" meaning.
    OwnUnits(Arc<[PathBuf]>),
}

/// Canonical paths of the units that belong to the source root `root`, in assembly order. A unit
/// under another effective root nested inside `root` belongs to that root, not to `root`.
pub(crate) fn own_unit_paths_under(
    units: &[SourceUnit],
    root: &Path,
    roots: &EffectiveCompilationRoots,
) -> Vec<PathBuf> {
    let root_key = crate::paths::unit_path_key(root);
    let nested_roots: Vec<PathBuf> = std::iter::once(&roots.host)
        .chain(roots.dependencies.iter())
        .map(|entry| crate::paths::unit_path_key(&entry.source_root))
        .filter(|other| *other != root_key && other.starts_with(&root_key))
        .collect();
    units
        .iter()
        .filter(|unit| {
            unit.path.starts_with(&root_key) && !nested_roots.iter().any(|nested| unit.path.starts_with(nested))
        })
        .map(|unit| unit.path.clone())
        .collect()
}

/// Generation-bound syntax project shared by analysis and IDE query boundaries.
#[derive(Clone)]
pub struct ProgramAssembly {
    /// Opaque source-issued contributions retained after the invocation closes.
    pub compiled_mod_metadata: Vec<crate::mod_host::ModCompiledMetadata>,
    pub verified_package_identities: super::VerifiedPackageIdentities,
    pub runtime_fixture: Option<Arc<beskid_abi::runtime_source::RuntimeFixtureProof>>,
    pub roots: EffectiveCompilationRoots,
    pub units: Arc<Vec<SourceUnit>>,
    /// Syntax indexes in exactly the same order as `units`.
    pub syntax_indexes: Arc<Vec<SyntaxIndex>>,
    pub generation: SyntaxGenerationId,
    pub entry_index: usize,
    /// Root units judged as the project's own code (see [`AssemblyRootSet`]).
    pub root_set: AssemblyRootSet,
    pub discovery: AssemblyDiscovery,
    pub recovery_policy: crate::projects::AssemblyRecoveryPolicy,
    pub module_index: Arc<ModuleIndex>,
    pub has_std_dependency: bool,
    /// Physical units copied from compiler-owned Foundation sources by workspace materialization.
    pub trusted_corelib_service_paths: Arc<[PathBuf]>,
    /// Owner library labels of the host manifest's `glue "<library>"` blocks. An `[Extern]`
    /// contract whose `Library` is one of them is a Rust Glue import validated by the Glue
    /// representation profile, not by the user C ABI profile. Empty for synthetic assemblies.
    pub glue_libraries: Arc<[String]>,
}

impl std::fmt::Debug for ProgramAssembly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramAssembly")
            .field("units", &self.units.len())
            .field("syntax_indexes", &self.syntax_indexes.len())
            .field("generation", &self.generation)
            .field("entry_index", &self.entry_index)
            .field("root_set", &self.root_set)
            .field("discovery", &self.discovery)
            .field("recovery_policy", &self.recovery_policy)
            .field("has_std_dependency", &self.has_std_dependency)
            .field("trusted_corelib_service_paths", &self.trusted_corelib_service_paths.len())
            .field("glue_libraries", &self.glue_libraries)
            .finish()
    }
}

impl ProgramAssembly {
    pub fn new(
        roots: EffectiveCompilationRoots,
        units: Arc<Vec<SourceUnit>>,
        entry_index: usize,
        discovery: AssemblyDiscovery,
        module_index: Arc<ModuleIndex>,
        has_std_dependency: bool,
        generation: SyntaxGenerationId,
    ) -> Self {
        let syntax_indexes =
            Arc::new(units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>());
        Self {
            compiled_mod_metadata: Vec::new(),
            verified_package_identities: Default::default(),
            runtime_fixture: None,
            roots,
            units,
            syntax_indexes,
            generation,
            entry_index,
            root_set: AssemblyRootSet::Entry,
            discovery,
            recovery_policy: crate::projects::AssemblyRecoveryPolicy::Strict,
            module_index,
            has_std_dependency,
            trusted_corelib_service_paths: Arc::from([]),
            glue_libraries: Arc::from([]),
        }
    }

    pub fn with_recovery_policy(mut self, policy: crate::projects::AssemblyRecoveryPolicy) -> Self {
        self.recovery_policy = policy;
        self
    }

    /// Carry an explicit root set onto a rebuilt assembly; [`ProgramAssembly::new`] starts from
    /// [`AssemblyRootSet::Entry`].
    pub fn with_root_set(mut self, root_set: AssemblyRootSet) -> Self {
        self.root_set = root_set;
        self
    }

    /// Indices of every root unit, entry first. Returns `None` when an [`AssemblyRootSet::OwnUnits`]
    /// path no longer names a unit of this assembly; callers fail closed on that.
    pub fn root_unit_indices(&self) -> Option<Vec<usize>> {
        match &self.root_set {
            AssemblyRootSet::Entry => Some(vec![self.entry_index]),
            AssemblyRootSet::OwnUnits(paths) => {
                let mut indices = vec![self.entry_index];
                for path in paths.iter() {
                    let index = self.units.iter().position(|unit| unit.path == *path)?;
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
                Some(indices)
            }
        }
    }

    /// Units whose code a library output of this assembly owns, entry first, then assembly order.
    ///
    /// For [`AssemblyRootSet::OwnUnits`] these are the root units. For [`AssemblyRootSet::Entry`]
    /// they are the entry plus every other unit of the host project's own source root (the units the
    /// entry imports from its own package), never a unit under a dependency root. Returns `None`
    /// when the root set names a unit outside this assembly; callers fail closed on that.
    pub fn library_unit_indices(&self) -> Option<Vec<usize>> {
        match &self.root_set {
            AssemblyRootSet::OwnUnits(_) => self.root_unit_indices(),
            AssemblyRootSet::Entry => {
                let mut indices = vec![self.entry_index];
                for path in own_unit_paths_under(&self.units, &self.roots.host.source_root, &self.roots) {
                    let index = self.units.iter().position(|unit| unit.path == path)?;
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
                Some(indices)
            }
        }
    }

    /// Root units other than the entry unit (empty for [`AssemblyRootSet::Entry`]).
    pub fn additional_root_indices(&self) -> Option<Vec<usize>> {
        let entry = self.entry_index;
        self.root_unit_indices().map(|indices| indices.into_iter().filter(|index| *index != entry).collect())
    }

    /// Whether `index` names a root unit.
    pub fn is_root_unit(&self, index: usize) -> bool {
        match &self.root_set {
            AssemblyRootSet::Entry => index == self.entry_index,
            AssemblyRootSet::OwnUnits(paths) => {
                index == self.entry_index || self.units.get(index).is_some_and(|unit| paths.contains(&unit.path))
            }
        }
    }

    /// The same assembly with `index` as the unit judged as entry (root-by-root checking).
    pub fn with_entry_index(&self, index: usize) -> Option<Self> {
        (index < self.units.len()).then(|| Self { entry_index: index, ..self.clone() })
    }

    pub fn package_identities(&self) -> &super::VerifiedPackageIdentities {
        &self.verified_package_identities
    }

    /// Whether `unit` is the canonical public Dynamic source: issued by the verified Corelib
    /// Foundation package at its canonical package-relative path with byte-exact canonical
    /// source. Its `[Extern]` bridges are compiler-issued runtime-plane bindings admitted by the
    /// semantic Dynamic bridge authority, not user C ABI imports.
    pub fn is_canonical_public_dynamic_unit(&self, unit: &SourceUnit) -> bool {
        self.package_identities().for_source(&unit.path).is_some_and(|root| {
            root.identity().package_name() == "corelib_foundation"
                && matches!(root.identity().source(), super::VerifiedPackageSource::Corelib)
                && root.relative_source_path(&unit.path).as_deref()
                    == Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
                && unit.source.as_bytes() == beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE.as_bytes()
        })
    }

    pub fn entry_unit(&self) -> &SourceUnit {
        &self.units[self.entry_index]
    }

    pub fn with_runtime_fixture(mut self, proof: Option<Arc<beskid_abi::runtime_source::RuntimeFixtureProof>>) -> Self {
        self.runtime_fixture = proof;
        self
    }

    pub fn entry_syntax_index(&self) -> &SyntaxIndex {
        &self.syntax_indexes[self.entry_index]
    }

    pub fn syntax_index_for_path(&self, path: &Path) -> Option<&SyntaxIndex> {
        self.units
            .iter()
            .position(|unit| crate::paths::same_file(&unit.path, path))
            .and_then(|index| self.syntax_indexes.get(index))
    }

    /// Rebind the entry unit when reusing a workspace-wide assembly for another target file.
    pub fn with_entry_at(&self, entry_path: &Path) -> Option<Self> {
        let target = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());
        let entry_index = self
            .units
            .iter()
            .position(|unit| unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone()) == target)?;
        Some(Self { entry_index, ..self.clone() })
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        roots::module_roots_from_effective(&self.roots)
    }

    /// Set the trusted corelib service paths (returns a new assembly via struct update).
    pub fn with_trusted_corelib_service_paths(mut self, paths: Arc<[PathBuf]>) -> Self {
        self.trusted_corelib_service_paths = paths;
        self
    }

    /// Set the host manifest's Glue owner library labels.
    pub fn with_glue_libraries(mut self, libraries: Arc<[String]>) -> Self {
        self.glue_libraries = libraries;
        self
    }

    /// Whether `library` names a `glue` owner block of the host manifest.
    pub fn is_glue_library(&self, library: &str) -> bool {
        self.glue_libraries.iter().any(|candidate| candidate == library)
    }
}
