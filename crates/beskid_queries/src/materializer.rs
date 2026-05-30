//! Bridge Salsa unit queries into program assembly.

use std::path::Path;
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::build_hir_units;
use beskid_analysis::projects::UnitMaterializer;

use crate::db::BeskidDatabase;
use crate::inputs::ProjectSession;
use crate::unit::parse_and_expand_unit;

pub fn unit_materializer_for(
    db: Arc<Mutex<BeskidDatabase>>,
    session: ProjectSession,
) -> UnitMaterializer {
    Arc::new(move |path: &Path, source: &str| {
        let mut guard = db.lock().expect("beskid database lock");
        guard.ensure_file_text(path.to_path_buf(), source.to_string());
        let unit = parse_and_expand_unit(&*guard, session, path.to_path_buf());
        let hir = build_hir_units(&[unit.clone()])
            .into_iter()
            .next()
            .expect("unit hir");
        Ok((unit, hir))
    })
}
