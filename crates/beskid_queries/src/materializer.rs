//! Bridge Salsa unit queries into program assembly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::UnitMaterializer;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;
use beskid_artifacts::content_fingerprint;

use crate::db::{BeskidDatabase, Db};
use crate::expand::expand_syntax_for_assembly;
use crate::inputs::ProjectSession;
use crate::stats::{record_query_hit, record_query_miss};

pub fn unit_materializer_for(db: Arc<Mutex<BeskidDatabase>>, session: ProjectSession) -> UnitMaterializer {
    Arc::new(move |path: &Path, source: &str, generation: SyntaxGenerationId| {
        let _ = session;
        let fp = content_fingerprint(source);
        if let Some(unit) = cached_unit(&db, &fp) {
            record_query_hit();
            let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
            return Ok((unit, syntax_index));
        }

        record_query_miss();
        let unit = parse_unit(path.to_path_buf(), source)?;
        let syntax_index = SyntaxIndex::from_program(&unit.program, generation);
        insert_cache(&db, fp, &unit);
        Ok((unit, syntax_index))
    })
}

fn cached_unit(db: &Arc<Mutex<BeskidDatabase>>, fp: &str) -> Option<beskid_analysis::projects::assembly::SourceUnit> {
    let guard = db.lock().expect("beskid database lock");
    let cache = guard.unit_cache().lock().expect("unit cache");
    Some(cache.source_units.get(fp)?.as_ref().clone())
}

fn insert_cache(db: &Arc<Mutex<BeskidDatabase>>, fp: String, unit: &beskid_analysis::projects::assembly::SourceUnit) {
    let guard = db.lock().expect("beskid database lock");
    let mut cache = guard.unit_cache().lock().expect("unit cache");
    cache.source_units.insert(fp, Arc::new(unit.clone()));
}

fn parse_unit(
    path: PathBuf,
    source: &str,
) -> Result<beskid_analysis::projects::assembly::SourceUnit, beskid_analysis::projects::AssemblyError> {
    let logical_name = path.display().to_string();
    let program =
        parse_program_with_source_name(&logical_name, source).map(expand_syntax_for_assembly).map_err(|err| {
            beskid_analysis::projects::AssemblyError::Parse { path: path.clone(), message: err.to_string() }
        })?;
    Ok(beskid_analysis::projects::assembly::SourceUnit { logical_name, path, source: source.to_string(), program })
}
