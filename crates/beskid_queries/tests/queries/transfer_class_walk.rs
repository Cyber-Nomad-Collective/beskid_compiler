//! The transfer-class walk classifies each nominal instantiation once per walk.
//!
//! Regression (0.6.7 hang): `beskid check` on the aggregate `corelib.bproj` never finished
//! because the walk re-classified a type once for every field path that reached it, which is
//! exponential in the depth of a diamond-shaped type graph.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, TransferClass, TransferNeverReason,
    nominal_transfer_class,
};

#[test]
fn a_diamond_shaped_type_graph_is_classified_within_a_time_bound() {
    // `Level0 { Level1 left; Level1 right; }` ... `LevelN { i32 value; }`: 2^N field paths.
    const DEPTH: usize = 32;
    let mut source = String::new();
    for level in 0..DEPTH {
        let next = level + 1;
        source.push_str(&format!("type Level{level} {{ Level{next} left; Level{next} right; }}\n"));
    }
    source.push_str(&format!("type Level{DEPTH} {{ i32 value; }}\n"));

    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/transfer-walk/src/Main.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/transfer-walk"),
        unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(3);
    let expanded = expand_program(parse_program(&source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let index = SyntaxIndex::from_program(&expanded, generation);
    db.ensure_file_text(unit.path(&db).clone(), source.clone());
    db.ensure_syntax_unit(project, unit, generation).expect("expanded syntax registration");
    let root = AstNodeKey {
        unit,
        generation,
        node: index.ids_named(NodeKind::TypeDefinition, "Level0").next().expect("Level0 declaration"),
    };

    let started = Instant::now();
    let class = nominal_transfer_class(&db, root, &[]).expect("transfer class");
    let elapsed = started.elapsed();

    // No level carries a marker, so the innermost type is NoMarker and every level above fails
    // on its first field.
    match &class {
        TransferClass::Never(TransferNeverReason::Field { type_name, field, .. }) => {
            assert_eq!(type_name, "Level0");
            assert_eq!(field, "left");
        }
        other => panic!("unexpected class {other:?}"),
    }
    assert!(elapsed < Duration::from_secs(10), "depth {DEPTH} took {elapsed:?}");
}
