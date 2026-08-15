use super::support::{assert_unavailable, key, key_at_start, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    call_arguments, call_lowering, collection_operation, direct_callees, nominal_member_receiver, reachable_items,
    resolved_item, SyntaxGenerationId,
};
use std::sync::Arc;

#[test]
fn call_lowering_classifies_immediate_lambda_without_name_resolution() {
    let source = "i64 Main() { return ((i64 value) => value)(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(call_lowering(&db, call).expect("lambda call lowering"), Some(beskid_queries::CallLowering::Dynamic));
}

#[test]
fn call_arguments_preserve_exact_root_keys_and_source_order() {
    let source = r#"i32 Target(i32 first, i32 second, i32 third) { return first; }
i32 Main() {
    let value = 4;
    Target(1, Target(2, 3, 4), value);
    Target();
    return 0;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let outer_call_offset = source.find("Target(1").expect("outer call");
    let nested_call_offset = source.find("Target(2").expect("nested call");
    let empty_call_offset = source.find("Target();").expect("empty call");
    let outer_call =
        key_at_start(unit, generation, &index, NodeKind::CallExpression, outer_call_offset + "Target".len());
    let empty_call =
        key_at_start(unit, generation, &index, NodeKind::CallExpression, empty_call_offset + "Target".len());
    let expected = [
        key_at_start(unit, generation, &index, NodeKind::Expression, source.find("1,").expect("first argument")),
        key_at_start(unit, generation, &index, NodeKind::Expression, nested_call_offset),
        key_at_start(unit, generation, &index, NodeKind::Expression, source.find("value);").expect("value argument")),
    ];

    assert_eq!(call_arguments(&db, outer_call).expect("outer arguments"), Some(Arc::from(expected)));
    assert_eq!(call_arguments(&db, empty_call).expect("empty arguments"), Some(Arc::from([])));
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_eq!(call_arguments(&db, main).expect("non-call"), None);

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(call_arguments(&db, outer_call).expect("stale arguments"), None);
}

#[test]
fn call_lowering_resolves_named_targets() {
    let source = r#"
i64 Helper() { return 1; }
i64 Main() { return Helper(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let named_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(call_lowering(&db, named_call).expect("named call"), Some(beskid_queries::CallLowering::Direct(helper)));
}

#[test]
fn collection_operation_denies_user_append_lookalikes() {
    let source = "i64 Append(i64 values, i64 value) { return values; } i64 Main() { return Append(1, 2); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(collection_operation(&db, call).expect("collection authority denial"), None);
}

#[test]
fn call_lowering_resolves_an_explicit_nominal_parameter_method() {
    let source = r#"
type Point { i32 x, i32 Ping() { return 7; } }
i32 Main(Point point) { return point.Ping(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let receiver = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("point) {").expect("parameter declaration"),
    );

    assert_eq!(
        call_lowering(&db, call).expect("nominal member call lowering"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(direct_callees(&db, main).expect("nominal member call graph"), Some(Arc::from([method])));
    assert_eq!(nominal_member_receiver(&db, receiver).expect("nominal receiver fact"), Some(declaration));
    assert_eq!(call_arguments(&db, call).expect("nominal member call arguments"), Some(Arc::from([receiver])));
}

#[test]
fn call_lowering_resolves_an_explicit_nominal_let_method() {
    let source = r#"
type Point { i32 x, i32 Ping() { return 7; } }
i32 Main() { Point point = Point { x: 1 }; return point.Ping(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let receiver = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("nominal let member call lowering"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(
        nominal_member_receiver(&db, receiver).expect("nominal let receiver fact"),
        Some(key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            source.find("point =").expect("let declaration"),
        ))
    );
}

#[test]
fn item_and_call_graph_facts_resolve_named_calls_and_recursion() {
    let source = r#"i32 Leaf() { return 1; }
i32 Recur(i32 count) {
    if count == 0 { return 0; }
    return Recur(count - 1);
}
i32 Main() {
    Leaf();
    return Recur(1);
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let leaf = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let recur = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    let leaf_call_offset = source.find("Leaf();").expect("leaf call");
    let leaf_path = key_at_start(unit, generation, &index, NodeKind::PathExpression, leaf_call_offset);
    let recursive_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let main_recur_call = key(unit, generation, &index, NodeKind::CallExpression, 2);

    assert_eq!(
        resolved_item(&db, leaf_path).expect("leaf item"),
        Some(beskid_queries::ResolvedItem { declaration: leaf })
    );
    assert_eq!(
        call_lowering(&db, recursive_call).expect("recursive lowering"),
        Some(beskid_queries::CallLowering::Direct(recur))
    );
    assert_eq!(
        call_lowering(&db, main_recur_call).expect("main recur lowering"),
        Some(beskid_queries::CallLowering::Direct(recur))
    );
    assert_eq!(direct_callees(&db, recur).expect("recursive callees"), Some(Arc::from([recur])));
    assert_eq!(direct_callees(&db, main).expect("main callees"), Some(Arc::from([leaf, recur])));
    let reachable = reachable_items(&db, program, main).expect("reachable query").expect("reachable facts");
    assert_eq!(reachable.as_ref(), &[main, leaf, recur]);
}

#[test]
fn reachable_items_includes_inline_method_callees_without_hir() {
    let source = "type Point { i32 x, i32 Ping() { return 7; } } i32 Main() { return Point { x: 1 }.Ping(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("inline method call"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(direct_callees(&db, main).expect("main callees"), Some(Arc::from([method])));
    assert_eq!(direct_callees(&db, method).expect("method callees"), Some(Arc::from([])));
    assert_eq!(
        reachable_items(&db, program, main).expect("reachable query").expect("reachable facts").as_ref(),
        &[main, method]
    );
}

#[test]
fn item_resolution_does_not_cross_local_shadowing_or_unresolved_names() {
    let shadowed_source = r#"i32 Helper() { return 1; }
i32 Main() {
    let Helper = (i32 value) => value;
    return Helper(1);
}"#;
    let (db, _project, unit, generation, index) = setup(shadowed_source);
    let shadowed_offset = shadowed_source.rfind("Helper(1)").expect("shadowed call");
    let shadowed_path = key_at_start(unit, generation, &index, NodeKind::PathExpression, shadowed_offset);
    let shadowed_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(resolved_item(&db, shadowed_path).expect("shadowed item"), None);
    assert_unavailable(call_lowering(&db, shadowed_call));

    let unresolved_source = "i32 Main() { return Missing(); }";
    let (db, _project, unit, generation, index) = setup(unresolved_source);
    let unresolved_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let unresolved_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let unresolved_main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let unresolved_program = key(unit, generation, &index, NodeKind::Program, 0);
    assert_eq!(resolved_item(&db, unresolved_path).expect("unresolved item"), None);
    assert_unavailable(call_lowering(&db, unresolved_call));
    // Unresolved calls are not Direct edges; reachability skips them instead of
    // failing the whole entrypoint walk (see direct_callees_for_item).
    assert_eq!(
        direct_callees(&db, unresolved_main).expect("no direct callees"),
        Some(std::sync::Arc::<[beskid_queries::AstNodeKey]>::from([]))
    );
    assert_eq!(
        reachable_items(&db, unresolved_program, unresolved_main).expect("entrypoint remains reachable"),
        Some(std::sync::Arc::from([unresolved_main]))
    );
}

#[test]
fn item_resolution_prefers_the_nearest_module_and_falls_back_lexically() {
    let source = r#"i32 Helper() { return 0; }
mod Inner {
    i32 Helper() { return 1; }
    i32 Main() { return Helper(); }
    i32 Fallback() { return OuterOnly(); }
}
i32 OuterOnly() { return 2; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let inner_helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let outer_only = key(unit, generation, &index, NodeKind::FunctionDefinition, 4);
    let inner_helper_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("Helper();").expect("inner helper call"),
    );
    let outer_only_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("OuterOnly();").expect("outer fallback call"),
    );

    assert_eq!(
        resolved_item(&db, inner_helper_path).expect("nearest module item"),
        Some(beskid_queries::ResolvedItem { declaration: inner_helper })
    );
    assert_eq!(
        resolved_item(&db, outer_only_path).expect("outer module fallback"),
        Some(beskid_queries::ResolvedItem { declaration: outer_only })
    );
}

#[test]
fn call_lowering_resolves_a_qualified_inline_module_function() {
    let source = r#"
mod Frame {
    pub i64 Repeat(i64 unit, i64 count) {
        mut i64 acc = 0;
        mut i64 i = 0;
        while i < count {
            acc = acc + unit;
            i = i + 1;
        }
        return acc;
    }
}
pub i64 Main() { return Frame.Repeat(1, 4); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let declaration = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        call_lowering(&db, call).expect("inline module call lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        direct_callees(&db, main).expect("inline module direct callee"),
        Some(std::sync::Arc::from([declaration]))
    );
}

#[test]
fn stale_generation_cannot_reuse_item_or_call_graph_facts() {
    let source = "i32 Helper() { return 1; } i32 Main() { return Helper(); }";
    let (mut db, project, unit, generation, index) = setup(source);
    let helper_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert!(resolved_item(&db, helper_path).expect("current item").is_some());
    assert!(direct_callees(&db, main).expect("current callees").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(resolved_item(&db, helper_path).expect("stale item"), None);
    assert_eq!(direct_callees(&db, main).expect("stale callees"), None);
}
