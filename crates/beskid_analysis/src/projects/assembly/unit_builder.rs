//! Unified unit materialization: Salsa in-memory cache + on-disk artifact store.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use beskid_artifacts::{ArtifactStore, content_fingerprint};

use crate::artifacts::{source_unit_from_ast_snapshot, source_unit_snapshot};
use crate::projects::assembly::loader::import_paths_from_program;

use super::SourceUnit;
use super::loader::expand_syntax_for_assembly;
use super::loader::{AssemblyError, MaterializedUnit, parse_program_for_unit};
use crate::projects::AssemblyRecoveryPolicy;
use crate::syntax::SyntaxGenerationId;
use crate::syntax_query::SyntaxIndex;

/// Expanded units of the toolchain's shared Corelib, keyed by canonical path, each with the
/// content fingerprint it was built from. Every project of one process reads the same shared
/// files, so a unit built for one project is reused by the next instead of being parsed and
/// expanded again. A unit is a pure function of its path and bytes (the on-disk artifact store is
/// content keyed for the same reason). The map holds one entry per file: a unit built from other
/// bytes (the bundle was replaced while a language server ran) evicts the stale one, so a long
/// session never accumulates versions. Units are shared behind `Arc`; the lock is held only to
/// clone the handle.
static SHARED_CORELIB_UNITS: LazyLock<Mutex<HashMap<PathBuf, (String, Arc<SourceUnit>)>>> =
    LazyLock::new(Default::default);

/// A Salsa unit build (see [`super::loader::UnitMaterializer`]).
type SalsaBuild<'a> = &'a (
        dyn Fn(&Path, &str, SyntaxGenerationId, AssemblyRecoveryPolicy) -> Result<MaterializedUnit, AssemblyError>
            + Send
            + Sync
    );

/// Builds expanded source and generation-bound syntax-index facts with artifact persistence.
///
/// Every cache it fills (the on-disk artifact store and the shared Corelib memo) holds only trees
/// of sources that parse, so a cache hit never needs a parse to prove the source is clean. Under
/// the strict policy a source that does not parse fails with its parse diagnostic.
pub struct UnitBuilder<'a> {
    _project_root: PathBuf,
    store: ArtifactStore,
    shared_corelib_root: Option<PathBuf>,
    recovery_policy: AssemblyRecoveryPolicy,
    salsa_build: Option<SalsaBuild<'a>>,
}

impl<'a> UnitBuilder<'a> {
    pub fn new(project_root: &Path) -> Self {
        Self {
            _project_root: project_root.to_path_buf(),
            store: ArtifactStore::new(project_root),
            shared_corelib_root: None,
            recovery_policy: AssemblyRecoveryPolicy::Strict,
            salsa_build: None,
        }
    }

    /// Keep the parser's repaired tree of a source that does not parse (editor tooling only).
    pub fn with_recovery_policy(mut self, policy: AssemblyRecoveryPolicy) -> Self {
        self.recovery_policy = policy;
        self
    }

    /// Reuse units below `root`, the canonical root of the shared Corelib, across assemblies.
    pub fn with_shared_corelib(mut self, root: Option<PathBuf>) -> Self {
        self.shared_corelib_root = root;
        self
    }

    pub fn with_salsa_build(mut self, build: SalsaBuild<'a>) -> Self {
        self.salsa_build = Some(build);
        self
    }

    pub fn build_unit(
        &self,
        path: &Path,
        source: &str,
        generation: SyntaxGenerationId,
    ) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> {
        self.build_assembly_unit(path, source, generation).map(|built| (built.unit, built.syntax_index))
    }

    /// Like [`Self::build_unit`], and says whether the tree is a parser repair (editor policy).
    pub fn build_assembly_unit(
        &self,
        path: &Path,
        source: &str,
        generation: SyntaxGenerationId,
    ) -> Result<MaterializedUnit, AssemblyError> {
        let fp = content_fingerprint(source);
        if !self.shared_corelib_root.as_deref().is_some_and(|root| path.starts_with(root)) {
            return self.build_unit_uncached(path, source, &fp, generation);
        }
        let reused = SHARED_CORELIB_UNITS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(path)
            .filter(|(built_from, _)| *built_from == fp)
            .map(|(_, unit)| Arc::clone(unit));
        if let Some(unit) = reused {
            crate::projects::assembly::unit_cache::record_shared_corelib_hit();
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            return Ok(MaterializedUnit { unit: SourceUnit::clone(&unit), syntax_index, recovered: false });
        }
        let built = self.build_unit_uncached(path, source, &fp, generation)?;
        if !built.recovered {
            SHARED_CORELIB_UNITS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(path.to_path_buf(), (fp, Arc::new(built.unit.clone())));
        }
        Ok(built)
    }

    fn build_unit_uncached(
        &self,
        path: &Path,
        source: &str,
        fp: &str,
        generation: SyntaxGenerationId,
    ) -> Result<MaterializedUnit, AssemblyError> {
        if let Some(ast_snap) = self.store.read_ast(fp)
            && ast_snap.meta.source_len == source.len()
            && let Ok(unit) = source_unit_from_ast_snapshot(&ast_snap, path, source)
        {
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            crate::projects::assembly::unit_cache::record_disk_hit();
            return Ok(MaterializedUnit { unit, syntax_index, recovered: false });
        }

        crate::projects::assembly::unit_cache::record_disk_miss();
        let built = match self.salsa_build {
            Some(build) => build(path, source, generation, self.recovery_policy)?,
            None => {
                let logical_name = path.display().to_string();
                let parsed = parse_program_for_unit(path, source, self.recovery_policy)?;
                let program = expand_syntax_for_assembly(parsed.program);
                let unit = SourceUnit::bind_request(path.to_path_buf(), logical_name, source.to_string(), program);
                let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
                MaterializedUnit { unit, syntax_index, recovered: parsed.recovered }
            }
        };
        // The store is content keyed, so a unit built through the Salsa cache is the same artifact
        // a plain build publishes; the next process reads it instead of parsing. A repaired tree
        // is never published: a later strict build would read it as the source's own tree.
        if !built.recovered {
            self.write_artifacts(&built.unit)?;
        }
        Ok(built)
    }

    fn write_artifacts(&self, unit: &SourceUnit) -> Result<(), AssemblyError> {
        let imports = import_paths_from_program(&unit.program.node);
        let ast = source_unit_snapshot(unit, &imports)
            .map_err(|err| AssemblyError::Artifact { path: unit.path.clone(), message: err.to_string() })?;
        if let Err(err) = self.store.write_unit(&ast) {
            log::warn!("failed to write unit artifact for {}: {err}", unit.path.display());
        }
        Ok(())
    }
}

#[cfg(test)]
mod shared_corelib_tests {
    use super::UnitBuilder;
    use crate::projects::assembly::disk_cache_stats;
    use crate::syntax::SyntaxGenerationId;
    use beskid_artifacts::content_fingerprint;

    #[test]
    fn a_shared_corelib_unit_is_built_once_per_process_and_reused_by_other_projects() {
        let toolchain = tempfile::tempdir().expect("toolchain");
        let shared = toolchain.path().canonicalize().unwrap().join("beskid_corelib/packages/text/src/Text.bd");
        std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
        let source = "pub i32 Length() { return 0; }\n";
        std::fs::write(&shared, source).unwrap();
        let first_project = tempfile::tempdir().expect("first project");
        let second_project = tempfile::tempdir().expect("second project");
        let root = Some(toolchain.path().canonicalize().unwrap());

        let before = disk_cache_stats().shared_corelib_hits;
        let (first, _) = UnitBuilder::new(first_project.path())
            .with_shared_corelib(root.clone())
            .build_unit(&shared, source, SyntaxGenerationId(1))
            .expect("first project builds the unit");
        let (second, _) = UnitBuilder::new(second_project.path())
            .with_shared_corelib(root)
            .build_unit(&shared, source, SyntaxGenerationId(1))
            .expect("second project reuses the unit");
        assert!(disk_cache_stats().shared_corelib_hits > before, "the second assembly reused the shared unit");
        assert_eq!(first.path, second.path);
        assert_eq!(first.source, second.source);

        // A unit outside the shared Corelib is never memoized.
        let local = first_project.path().join("Main.bd");
        std::fs::write(&local, source).unwrap();
        UnitBuilder::new(first_project.path())
            .with_shared_corelib(Some(toolchain.path().canonicalize().unwrap()))
            .build_unit(&local, source, SyntaxGenerationId(1))
            .expect("local unit");
        {
            let memo = super::SHARED_CORELIB_UNITS.lock().unwrap();
            assert!(memo.contains_key(&shared));
            assert!(!memo.contains_key(&local), "a project unit is never shared");
        }

        // Other bytes for the same shared file (a replaced bundle) evict the stale unit.
        let edited = "pub i32 Length() { return 2; }\n";
        let (rebuilt, _) = UnitBuilder::new(second_project.path())
            .with_shared_corelib(Some(toolchain.path().canonicalize().unwrap()))
            .build_unit(&shared, edited, SyntaxGenerationId(2))
            .expect("rebuilt unit");
        assert_eq!(&*rebuilt.source, edited);
        let memo = super::SHARED_CORELIB_UNITS.lock().unwrap();
        let (built_from, unit) = memo.get(&shared).expect("one entry per shared file");
        assert_eq!(built_from, &content_fingerprint(edited));
        assert_eq!(&*unit.source, edited);
    }
}
