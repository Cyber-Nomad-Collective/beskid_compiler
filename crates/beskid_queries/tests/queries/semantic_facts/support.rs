use std::path::PathBuf;

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{AstNodeKey, BeskidDatabase, ProjectSession, SemanticError, SourceUnitId, SyntaxGenerationId};

pub(super) fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    let error = match result {
        Ok(_) => panic!("current unported semantic query must fail explicitly"),
        Err(error) => error,
    };
    assert!(error.is_unavailable(), "{error:?}");
}

pub(super) fn setup(source: &str) -> (BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, SyntaxIndex) {
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Main.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(3);
    let expanded = expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let index = SyntaxIndex::from_program(&expanded, generation);
    db.ensure_file_text(unit.path(&db).clone(), source.to_string());
    db.ensure_syntax_unit(project, unit, generation).expect("expanded syntax registration");
    (db, project, unit, generation, index)
}

pub(super) fn key(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    occurrence: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .ids_of_kind(kind)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {kind:?} occurrence {occurrence}")),
    }
}

pub(super) fn key_at_start(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    start: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .metadata()
            .iter()
            .find(|metadata| metadata.kind == kind && metadata.span.is_some_and(|span| span.start == start))
            .unwrap_or_else(|| panic!("missing {kind:?} at byte {start}"))
            .id,
    }
}
