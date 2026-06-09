//! Unified unit materialization: Salsa in-memory cache + on-disk artifact store.

use std::path::{Path, PathBuf};

use beskid_artifacts::{ArtifactStore, content_fingerprint};

use crate::artifacts::{
    hir_unit_snapshot, source_unit_from_ast_snapshot, source_unit_snapshot,
    unit_hir_from_hir_snapshot,
};
use crate::projects::assembly::loader::import_paths_from_source_full;

use super::loader::AssemblyError;
use super::loader::expand_syntax_for_assembly;
use super::{SourceUnit, UnitHir, build_hir_units};

/// Builds `(SourceUnit, UnitHir)` with artifact persistence and optional Salsa delegate.
pub struct UnitBuilder<'a> {
    _project_root: PathBuf,
    store: ArtifactStore,
    salsa_build: Option<
        &'a (dyn Fn(&Path, &str) -> Result<(SourceUnit, UnitHir), AssemblyError> + Send + Sync),
    >,
}

impl<'a> UnitBuilder<'a> {
    pub fn new(project_root: &Path) -> Self {
        Self {
            _project_root: project_root.to_path_buf(),
            store: ArtifactStore::new(project_root),
            salsa_build: None,
        }
    }

    pub fn with_salsa_build(
        mut self,
        build: &'a (
                dyn Fn(&Path, &str) -> Result<(SourceUnit, UnitHir), AssemblyError> + Send + Sync
            ),
    ) -> Self {
        self.salsa_build = Some(build);
        self
    }

    pub fn build_unit(
        &self,
        path: &Path,
        source: &str,
    ) -> Result<(SourceUnit, UnitHir), AssemblyError> {
        let fp = content_fingerprint(source);
        if let (Some(ast_snap), Some(hir_snap)) =
            (self.store.read_ast(&fp), self.store.read_hir(&fp))
            && ast_snap.meta.source_len == source.len()
                && let Ok(unit) = source_unit_from_ast_snapshot(&ast_snap, source)
                && let Ok(hir) = unit_hir_from_hir_snapshot(path.to_path_buf(), &unit, &hir_snap)
            {
                crate::projects::assembly::unit_cache::record_disk_hit();
                return Ok((unit, hir));
            }

        if let Some(build) = self.salsa_build {
            crate::projects::assembly::unit_cache::record_disk_miss();
            return build(path, source);
        }

        crate::projects::assembly::unit_cache::record_disk_miss();
        let logical_name = path.display().to_string();
        let program = crate::services::parse_program_with_source_name(&logical_name, source)
            .map(expand_syntax_for_assembly)
            .map_err(|err| AssemblyError::Parse {
                path: path.to_path_buf(),
                message: err.to_string(),
            })?;
        let unit = SourceUnit {
            logical_name,
            path: crate::paths::unit_path_key(path),
            source: source.to_string(),
            program,
        };
        let hir = build_hir_units(std::slice::from_ref(&unit))
            .into_iter()
            .next()
            .expect("unit hir");
        self.write_artifacts(&unit, &hir, source)?;
        Ok((unit, hir))
    }

    fn write_artifacts(
        &self,
        unit: &SourceUnit,
        hir: &UnitHir,
        source: &str,
    ) -> Result<(), AssemblyError> {
        let fp = content_fingerprint(source);
        let imports = import_paths_from_source_full(source);
        let ast = source_unit_snapshot(unit, &imports).map_err(|err| AssemblyError::Parse {
            path: unit.path.clone(),
            message: err.to_string(),
        })?;
        let hir_snap = hir_unit_snapshot(&fp, hir).map_err(|err| AssemblyError::Parse {
            path: unit.path.clone(),
            message: err.to_string(),
        })?;
        if let Err(err) = self.store.write_unit(&ast, &hir_snap) {
            log::warn!(
                "failed to write unit artifact for {}: {err}",
                unit.path.display()
            );
        }
        Ok(())
    }
}
