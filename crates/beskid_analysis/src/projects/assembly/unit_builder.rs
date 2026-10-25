//! Unified unit materialization: Salsa in-memory cache + on-disk artifact store.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use beskid_artifacts::{ArtifactStore, content_fingerprint};

use crate::artifacts::{source_unit_from_ast_snapshot, source_unit_snapshot};
use crate::projects::assembly::loader::import_paths_from_program;

use super::SourceUnit;
use super::loader::AssemblyError;
use super::loader::expand_syntax_for_assembly;
use crate::syntax::SyntaxGenerationId;
use crate::syntax_query::SyntaxIndex;

/// Expanded units of the toolchain's shared Corelib, keyed by canonical path and content
/// fingerprint. Every project of one process reads the same shared files, so a unit built for one
/// project is reused by the next instead of being parsed and expanded again. A unit is a pure
/// function of its path and bytes (the on-disk artifact store is content keyed for the same
/// reason), and the shared Corelib is sealed, so the map holds at most one entry per file.
static SHARED_CORELIB_UNITS: LazyLock<Mutex<HashMap<(PathBuf, String), SourceUnit>>> = LazyLock::new(Default::default);

/// Builds expanded source and generation-bound syntax-index facts with artifact persistence.
pub struct UnitBuilder<'a> {
    _project_root: PathBuf,
    store: ArtifactStore,
    shared_corelib_root: Option<PathBuf>,
    salsa_build: Option<
        &'a (dyn Fn(&Path, &str, SyntaxGenerationId) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> + Send + Sync),
    >,
}

impl<'a> UnitBuilder<'a> {
    pub fn new(project_root: &Path) -> Self {
        Self {
            _project_root: project_root.to_path_buf(),
            store: ArtifactStore::new(project_root),
            shared_corelib_root: None,
            salsa_build: None,
        }
    }

    /// Reuse units below `root`, the canonical root of the shared Corelib, across assemblies.
    pub fn with_shared_corelib(mut self, root: Option<PathBuf>) -> Self {
        self.shared_corelib_root = root;
        self
    }

    pub fn with_salsa_build(
        mut self,
        build: &'a (
                dyn Fn(&Path, &str, SyntaxGenerationId) -> Result<(SourceUnit, SyntaxIndex), AssemblyError>
                    + Send
                    + Sync
            ),
    ) -> Self {
        self.salsa_build = Some(build);
        self
    }

    pub fn build_unit(
        &self,
        path: &Path,
        source: &str,
        generation: SyntaxGenerationId,
    ) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> {
        let fp = content_fingerprint(source);
        let shared_key = self
            .shared_corelib_root
            .as_deref()
            .filter(|root| path.starts_with(root))
            .map(|_| (path.to_path_buf(), fp.clone()));
        let Some(shared_key) = shared_key else {
            return self.build_unit_uncached(path, source, &fp, generation);
        };
        let reused =
            SHARED_CORELIB_UNITS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&shared_key).cloned();
        if let Some(unit) = reused {
            crate::projects::assembly::unit_cache::record_shared_corelib_hit();
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            return Ok((unit, syntax_index));
        }
        let (unit, syntax_index) = self.build_unit_uncached(path, source, &fp, generation)?;
        SHARED_CORELIB_UNITS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(shared_key, unit.clone());
        Ok((unit, syntax_index))
    }

    fn build_unit_uncached(
        &self,
        path: &Path,
        source: &str,
        fp: &str,
        generation: SyntaxGenerationId,
    ) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> {
        if let Some(ast_snap) = self.store.read_ast(fp)
            && ast_snap.meta.source_len == source.len()
            && let Ok(unit) = source_unit_from_ast_snapshot(&ast_snap, path, source)
        {
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            crate::projects::assembly::unit_cache::record_disk_hit();
            return Ok((unit, syntax_index));
        }

        if let Some(build) = self.salsa_build {
            crate::projects::assembly::unit_cache::record_disk_miss();
            let (unit, syntax_index) = build(path, source, generation)?;
            // The store is content keyed, so a unit built through the Salsa cache is the same
            // artifact a plain build publishes; the next process reads it instead of parsing.
            self.write_artifacts(&unit)?;
            return Ok((unit, syntax_index));
        }

        crate::projects::assembly::unit_cache::record_disk_miss();
        let logical_name = path.display().to_string();
        let program = crate::services::parse_program_with_source_name(&logical_name, source)
            .map(expand_syntax_for_assembly)
            .map_err(|err| AssemblyError::from_parse_failure(path.to_path_buf(), source, err))?;
        let unit = SourceUnit::bind_request(path.to_path_buf(), logical_name, source.to_string(), program);
        let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
        self.write_artifacts(&unit)?;
        Ok((unit, syntax_index))
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
        let memo = super::SHARED_CORELIB_UNITS.lock().unwrap();
        assert!(memo.keys().any(|(path, _)| path == &shared));
        assert!(memo.keys().all(|(path, _)| path != &local), "a project unit is never shared");
    }
}
