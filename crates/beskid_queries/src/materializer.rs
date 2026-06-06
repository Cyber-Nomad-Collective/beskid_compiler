//! Bridge Salsa unit queries into program assembly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::{UnitMaterializer, build_hir_units};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_artifacts::content_fingerprint;

use crate::db::{BeskidDatabase, Db};
use crate::expand::expand_syntax_for_assembly;
use crate::inputs::ProjectSession;
use crate::stats::{record_query_hit, record_query_miss};

pub fn unit_materializer_for(
    db: Arc<Mutex<BeskidDatabase>>,
    session: ProjectSession,
) -> UnitMaterializer {
    Arc::new(move |path: &Path, source: &str| {
        let _ = session;
        let fp = content_fingerprint(source);
        if let Some((unit, hir)) = cached_pair(&db, &fp) {
            record_query_hit();
            return Ok((unit, hir));
        }

        record_query_miss();
        let unit = parse_unit(path.to_path_buf(), source)?;
        let hir = build_hir_units(std::slice::from_ref(&unit))
            .into_iter()
            .next()
            .expect("unit hir");
        insert_cache(&db, fp, &unit, hir);
        let hir = build_hir_units(std::slice::from_ref(&unit))
            .into_iter()
            .next()
            .expect("unit hir");
        Ok((unit, hir))
    })
}

fn cached_pair(
    db: &Arc<Mutex<BeskidDatabase>>,
    fp: &str,
) -> Option<(
    beskid_analysis::projects::assembly::SourceUnit,
    beskid_analysis::projects::assembly::UnitHir,
)> {
    let guard = db.lock().expect("beskid database lock");
    let cache = guard.unit_cache().lock().expect("unit cache");
    let unit = cache.source_units.get(fp)?.as_ref().clone();
    let hir_arc = Arc::clone(cache.unit_hir.get(fp)?);
    let hir = Arc::try_unwrap(hir_arc).unwrap_or_else(|_| {
        build_hir_units(std::slice::from_ref(&unit))
            .into_iter()
            .next()
            .expect("unit hir")
    });
    Some((unit, hir))
}

fn insert_cache(
    db: &Arc<Mutex<BeskidDatabase>>,
    fp: String,
    unit: &beskid_analysis::projects::assembly::SourceUnit,
    hir: beskid_analysis::projects::assembly::UnitHir,
) {
    let guard = db.lock().expect("beskid database lock");
    let mut cache = guard.unit_cache().lock().expect("unit cache");
    cache
        .source_units
        .insert(fp.clone(), Arc::new(unit.clone()));
    cache.unit_hir.insert(fp, Arc::new(hir));
}

fn parse_unit(
    path: PathBuf,
    source: &str,
) -> Result<beskid_analysis::projects::assembly::SourceUnit, beskid_analysis::projects::AssemblyError>
{
    let logical_name = path.display().to_string();
    let program = parse_program_with_source_name(&logical_name, source)
        .map(expand_syntax_for_assembly)
        .map_err(|err| beskid_analysis::projects::AssemblyError::Parse {
            path: path.clone(),
            message: err.to_string(),
        })?;
    Ok(beskid_analysis::projects::assembly::SourceUnit {
        logical_name,
        path,
        source: source.to_string(),
        program,
    })
}
