//! Bridge Salsa unit queries into program assembly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::{SourceUnit, UnitMaterializer};
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
            let unit = SourceUnit::bind_request(
                path.to_path_buf(),
                path.display().to_string(),
                source.to_string(),
                unit.program,
            );
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
    Ok(SourceUnit::bind_request(path, logical_name, source.to_string(), program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializer_rebinds_content_cache_origin_in_both_request_orders() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("Real.bd");
        let copy = root.path().join("Copy.bd");
        let source = "i32 Main() { return 0; }";
        std::fs::write(&real, source).unwrap();
        std::fs::write(&copy, source).unwrap();
        let real = real.canonicalize().unwrap();
        let copy = copy.canonicalize().unwrap();
        for paths in [[&real, &copy, &real], [&copy, &real, &copy]] {
            let db = BeskidDatabase::default();
            let session = ProjectSession::new(&db, root.path().into(), real.clone(), "App".into(), "lock".into());
            let db = Arc::new(Mutex::new(db));
            let materialize = unit_materializer_for(db.clone(), session);
            for (index, path) in paths.into_iter().enumerate() {
                let generation = SyntaxGenerationId(index as u64 + 1);
                let (unit, syntax) = materialize(path, source, generation).unwrap();
                assert_eq!(&unit.origin_path, path);
                assert_eq!(&unit.path, path);
                assert_eq!(unit.logical_name, path.display().to_string());
                assert_eq!(syntax.generation(), generation);
                assert_eq!(db.lock().unwrap().unit_cache().lock().unwrap().source_units.len(), 1);
            }
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn materializer_and_public_queries_bind_symlink_identity_on_cold_and_warm_requests() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("Real.bd");
        let alias = root.path().join("Alias.bd");
        let source = "i32 Main() { return 0; }";
        std::fs::write(&real, source).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &alias).expect("create source symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real, &alias).expect("create source symlink");
        let real = real.canonicalize().unwrap();
        for route in 0..3 {
            for paths in [[&real, &alias, &real], [&alias, &real, &alias]] {
                let db = BeskidDatabase::default();
                let session = ProjectSession::new(&db, root.path().into(), real.clone(), "App".into(), "lock".into());
                let db = Arc::new(Mutex::new(db));
                let materialize = unit_materializer_for(db.clone(), session);
                for path in paths {
                    let unit = match route {
                        0 => materialize(path, source, SyntaxGenerationId(1)).unwrap().0,
                        1 => crate::parse_and_expand_unit(&*db.lock().unwrap(), session, path.clone()),
                        _ => crate::parse_and_expand_unit_with_source(
                            &*db.lock().unwrap(),
                            session,
                            path.clone(),
                            source,
                        ),
                    };
                    assert_eq!(&unit.origin_path, path, "route={route}");
                    assert_eq!(unit.path, real, "canonical key must not retain symlink spelling");
                    assert_eq!(unit.logical_name, path.display().to_string());
                    assert_eq!(db.lock().unwrap().unit_cache().lock().unwrap().source_units.len(), 1);
                }
            }
        }
    }
}
