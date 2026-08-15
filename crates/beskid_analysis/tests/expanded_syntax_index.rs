use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};

#[test]
fn expanded_syntax_index_is_deterministic_preorder() {
    let program = parse_program("i32 Main() { let value = 1; return value; }").expect("program parses");

    let first = SyntaxIndex::from_program(&program, SyntaxGenerationId(7));
    let second = SyntaxIndex::from_program(&program, SyntaxGenerationId(7));

    assert_eq!(first.metadata(), second.metadata());
    assert_eq!(first.generation(), SyntaxGenerationId(7));
    assert_eq!(first.kind(AstNodeId(0)), Some(NodeKind::Program));
    assert_eq!(
        first.children(AstNodeId(0)).expect("root children").len(),
        1,
        "the index owns direct child relationships without rebuilding a snapshot"
    );
    assert!(first.len() > 1);
    for metadata in first.metadata() {
        assert_eq!(first.node_at(&program, metadata.id).expect("indexed lookup").node_kind(), metadata.kind);
    }
}

#[test]
fn expanded_syntax_index_projects_expanded_imports_and_modules() {
    let program = parse_program("use Core.Text.Regex; pub mod Core.Text.Generated; mod Local { use Core.IO; }")
        .expect("program parses");
    let index = SyntaxIndex::from_program(&program, SyntaxGenerationId(10));

    assert_eq!(
        index.import_paths(&program),
        vec![
            vec!["Core".to_string(), "Text".to_string(), "Regex".to_string()],
            vec!["Core".to_string(), "IO".to_string()],
        ]
    );
    assert_eq!(
        index.module_declaration_paths(&program),
        vec![vec!["Core".to_string(), "Text".to_string(), "Generated".to_string()]]
    );
    assert_eq!(index.inline_module_names(&program), vec!["Local".to_string()]);
}

#[test]
fn expanded_syntax_index_rejects_stale_generation() {
    let program = parse_program("i32 Main() { return 0; }").expect("program parses");
    let index = SyntaxIndex::from_program(&program, SyntaxGenerationId(9));

    assert!(index.metadata_for(SyntaxGenerationId(8), AstNodeId(0)).is_none());
    assert_eq!(index.metadata_for(SyntaxGenerationId(9), AstNodeId(0)).expect("current root").kind, NodeKind::Program);
}
