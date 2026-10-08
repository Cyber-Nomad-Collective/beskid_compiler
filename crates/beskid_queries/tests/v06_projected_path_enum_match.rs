//! A match scrutinee that is a projected path (`value.node.literal.node`, or `binding.field`
//! rooted in an enum pattern binding) proves its enum through the nominal field projection.
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SourceUnitId, build_typed_program, enum_match,
    project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

const SOURCE: &str = r#"
enum Literal { Str(string raw), Int(string digits), }
type SpannedLiteral { Literal node, u32 id, }
type LiteralExpression { SpannedLiteral literal, }
type SpannedLiteralExpression { LiteralExpression node, u32 id, }
enum Expression { Literal(SpannedLiteralExpression value), Other, }
type SpannedExpression { Expression node, u32 id, }
string Text(SpannedExpression expression) {
    match expression.node {
        Expression::Literal(value) => {
            match value.node.literal.node {
                Literal::Str(raw) => { return raw; },
                _ => { },
            };
        },
        _ => { },
    };
    return "";
}
"#;

fn fixture(source: &str) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>, Vec<AstNodeKey>) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("src");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let plan = CompilePlan {
        project_root: temporary.path().to_owned(),
        manifest_path: temporary.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "projected-match".into()).unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let matches = assembly
        .entry_syntax_index()
        .ids_of_kind(NodeKind::MatchExpression)
        .map(|node| AstNodeKey { unit, generation: assembly.generation, node })
        .collect();
    (temporary, db, assembly, matches)
}

#[test]
fn projected_path_scrutinee_rooted_in_a_pattern_binding_proves_its_enum() {
    let (_temporary, db, _assembly, matches) = fixture(SOURCE);
    assert_eq!(matches.len(), 2);
    let outer = enum_match(&db, matches[0]).expect("outer match fact").expect("outer field scrutinee");
    assert_eq!(outer.arms.len(), 2);
    let inner = enum_match(&db, matches[1]).expect("inner match fact").expect("`value.node.literal.node` scrutinee");
    assert_eq!(inner.arms.len(), 2, "the projected `Literal` enum supplies both arms");
    assert_ne!(outer.declaration, inner.declaration, "the inner match is on `Literal`, not `Expression`");
}

#[test]
fn unknown_projected_field_keeps_the_match_unavailable() {
    let (_temporary, db, _assembly, matches) =
        fixture(&SOURCE.replace("match value.node.literal.node", "match value.node.missing.node"));
    assert!(!matches!(enum_match(&db, matches[1]), Ok(Some(_))), "an unproven projection never yields a match fact");
}
