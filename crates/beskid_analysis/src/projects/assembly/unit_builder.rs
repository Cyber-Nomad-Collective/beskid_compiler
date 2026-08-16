//! Unified unit materialization: Salsa in-memory cache + on-disk artifact store.

use std::path::{Path, PathBuf};

use beskid_artifacts::{ArtifactStore, content_fingerprint};

use crate::artifacts::{source_unit_from_ast_snapshot, source_unit_snapshot};
use crate::projects::assembly::loader::import_paths_from_source_full;

use super::SourceUnit;
use super::loader::AssemblyError;
use super::loader::expand_syntax_for_assembly;
use crate::syntax::SyntaxGenerationId;
use crate::syntax_query::SyntaxIndex;

/// Builds expanded source and generation-bound syntax-index facts with artifact persistence.
pub struct UnitBuilder<'a> {
    _project_root: PathBuf,
    store: ArtifactStore,
    salsa_build: Option<
        &'a (dyn Fn(&Path, &str, SyntaxGenerationId) -> Result<(SourceUnit, SyntaxIndex), AssemblyError> + Send + Sync),
    >,
}

impl<'a> UnitBuilder<'a> {
    pub fn new(project_root: &Path) -> Self {
        Self { _project_root: project_root.to_path_buf(), store: ArtifactStore::new(project_root), salsa_build: None }
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
        if let Some(ast_snap) = self.store.read_ast(&fp)
            && ast_snap.meta.source_len == source.len()
            && let Ok(unit) = source_unit_from_ast_snapshot(&ast_snap, source)
        {
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            crate::projects::assembly::unit_cache::record_disk_hit();
            return Ok((unit, syntax_index));
        }

        if let Some(build) = self.salsa_build {
            crate::projects::assembly::unit_cache::record_disk_miss();
            return build(path, source, generation);
        }

        crate::projects::assembly::unit_cache::record_disk_miss();
        let logical_name = path.display().to_string();
        let program = crate::services::parse_program_with_source_name(&logical_name, source)
            .map(expand_syntax_for_assembly)
            .map_err(|err| AssemblyError::Parse { path: path.to_path_buf(), message: err.to_string() })?;
        let unit =
            SourceUnit { logical_name, path: crate::paths::unit_path_key(path), source: source.to_string(), program };
        let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
        self.write_artifacts(&unit, source)?;
        Ok((unit, syntax_index))
    }

    fn write_artifacts(&self, unit: &SourceUnit, source: &str) -> Result<(), AssemblyError> {
        let imports = import_paths_from_source_full(source);
        let ast = source_unit_snapshot(unit, &imports)
            .map_err(|err| AssemblyError::Parse { path: unit.path.clone(), message: err.to_string() })?;
        if let Err(err) = self.store.write_unit(&ast) {
            log::warn!("failed to write unit artifact for {}: {err}", unit.path.display());
        }
        Ok(())
    }
}
