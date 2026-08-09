use super::support::{key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{AstNodeKey, CompletionContext, SyntaxGenerationId, completion_candidates};

#[test]
fn completion_candidates_are_generation_safe_and_deterministic() {
    let source = "i32 Zebra() { return 0; } i32 Alpha() { return Zebra(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let cursor = source.find("Zebra();").expect("call");
    let candidates = completion_candidates(
        &db,
        program,
        CompletionContext { cursor, replacement_start: cursor, replacement_end: cursor + 1 },
    )
    .expect("completion")
    .expect("current generation");
    assert_eq!(candidates.iter().map(|candidate| candidate.label.as_ref()).collect::<Vec<_>>(), vec!["Zebra"]);
    assert_eq!((candidates[0].replacement_start, candidates[0].replacement_end), (cursor, cursor + 1));
    assert_eq!(
        completion_candidates(
            &db,
            AstNodeKey { generation: SyntaxGenerationId(generation.0 - 1), ..program },
            CompletionContext { cursor, replacement_start: cursor, replacement_end: cursor }
        ),
        Ok(None)
    );
    let unicode = "i32 Main() { return \"é\"; }";
    let (db, _project, unit, generation, index) = setup(unicode);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let invalid = unicode.find('é').expect("unicode") + 1;
    assert_eq!(
        completion_candidates(
            &db,
            program,
            CompletionContext { cursor: invalid, replacement_start: invalid, replacement_end: invalid }
        ),
        Ok(None)
    );
}

#[test]
fn completion_candidates_cover_lexical_type_and_receiver_families() {
    let source = r#"type Value { i32 raw
i32 Sum(i32 first) { return first + raw; }
}
i32 Helper() { return 1; }
i32 Main(Value value) {
    let amount = 2;
    return value.Su;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);

    let lexical_cursor = source.find("return value").expect("lexical site");
    let lexical = completion_candidates(
        &db,
        program,
        CompletionContext {
            cursor: lexical_cursor,
            replacement_start: lexical_cursor,
            replacement_end: lexical_cursor,
        },
    )
    .expect("lexical completion")
    .expect("current generation");
    let lexical_labels = lexical.iter().map(|candidate| (candidate.label.as_ref(), candidate.kind)).collect::<Vec<_>>();
    assert!(
        lexical_labels.contains(&("amount", beskid_queries::CompletionKind::Variable)),
        "expected lexical local amount, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("value", beskid_queries::CompletionKind::Variable)),
        "expected lexical parameter value, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("Value", beskid_queries::CompletionKind::Type)),
        "expected type candidate Value, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("Helper", beskid_queries::CompletionKind::Function)),
        "expected function candidate Helper, got {lexical_labels:?}"
    );

    let receiver_cursor = source.find("value.Su").expect("receiver site") + "value.".len();
    let receiver = completion_candidates(
        &db,
        program,
        CompletionContext {
            cursor: receiver_cursor,
            replacement_start: receiver_cursor,
            replacement_end: receiver_cursor + "Su".len(),
        },
    )
    .expect("receiver completion")
    .expect("receiver candidates");
    assert_eq!(
        receiver.iter().map(|candidate| (candidate.label.as_ref(), candidate.kind)).collect::<Vec<_>>(),
        vec![("Sum", beskid_queries::CompletionKind::Method)]
    );

    let inferred = "type Value { i32 raw }\ni32 Main() { let value = 1; return value.x; }";
    let (db, _project, unit, generation, index) = setup(inferred);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let inferred_cursor = inferred.find("value.x").expect("inferred site") + "value.".len();
    assert_eq!(
        completion_candidates(
            &db,
            program,
            CompletionContext {
                cursor: inferred_cursor,
                replacement_start: inferred_cursor,
                replacement_end: inferred_cursor + 1,
            }
        ),
        Ok(None),
        "inferred or non-nominal receivers remain unavailable"
    );
}
