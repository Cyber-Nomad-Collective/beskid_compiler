use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CastIntent, CodegenInput,
    EffectiveCompilationRoots, FunctionEmitter, ModuleIndex, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, SyntaxProgramAssembly, TargetMetadata, UserFuncName, build_typed_program,
    closure_environment, emit_isle_expression, find_function_definition, find_function_definitions,
    find_integer_literal, find_node, format_ast_node_site, isa, item_body, item_fixture_with_root,
    lower_syntax_program, node_kind, parse_program_with_source_name, settings, spawn_target, types,
};

#[test]
fn parsed_syntax_root_emits_verified_isle_clif_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 42; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source).expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let literal = find_integer_literal(&db, root).expect("integer literal key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64").expect("host ISA").finish(flags).expect("host flags");

    let function = emit_isle_expression(&input, isa.as_ref(), literal, types::I32)
        .expect("parsed expression lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}
#[test]
fn parsed_multi_function_assembly_verification_error_identifies_the_originating_item_site() {
    let (input, isa, root) = item_fixture_with_root("i32 Sibling() { return 1; } i32 Failing() { 2; }");
    let db = input.database();
    let items = find_function_definitions(db, root);
    let sibling = items[0];
    let failing = items[1];

    let error = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: sibling, symbol: "Sibling".into() },
            SyntaxModuleItem { key: failing, symbol: "Failing".into() },
        ],
    )
    .expect_err("the failing item must be rejected through module emission");
    let first = error.to_string();
    let repeated = error.to_string();
    let failing_site = format_ast_node_site(db, failing);
    let sibling_site = format_ast_node_site(db, sibling);

    assert_eq!(first, repeated);
    assert!(first.contains(&failing_site), "{first}");
    assert!(!first.contains(&sibling_site), "{first}");
    assert!(first.contains("syntax ISLE emission failed: Verification("), "{first}");
    assert!(first.contains("FunctionDefinition@"), "{first}");
}

#[test]
fn parsed_statement_final_block_error_identifies_the_originating_body_site() {
    let (input, isa, root) = item_fixture_with_root("unit Main() { 2; }");
    let db = input.database();
    let body = find_node(db, root, beskid_queries::IndexedNodeKind::ExpressionStatement).expect("expression statement");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let emitter = FunctionEmitter::new(isa.as_ref());

    let error = emitter
        .emit_statement(UserFuncName::user(0, 100), emitter.signature([], [types::I32]), &facts, body)
        .expect_err("non-unit fallthrough must fail final-block verification");
    let rendered = error.display_with_db(db);

    assert!(rendered.contains(&format_ast_node_site(db, body)), "{rendered}");
    assert!(rendered.contains("ExpressionStatement@"), "{rendered}");
}

#[test]
fn parsed_parameter_materialization_error_identifies_the_originating_item_site() {
    let (input, isa, root) = item_fixture_with_root("i32 Main(i32 value) { return value; }");
    let db = input.database();
    let item = find_function_definition(db, root).expect("function item");
    let body = item_body(db, item).expect("body query").expect("function body");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let emitter = FunctionEmitter::new(isa.as_ref());

    let error = emitter
        .emit_item_statement(UserFuncName::user(0, 101), emitter.signature([], [types::I32]), &facts, item, body)
        .expect_err("missing incoming parameter must fail materialization");
    let rendered = error.display_with_db(db);

    assert!(rendered.contains(&format_ast_node_site(db, item)), "{rendered}");
    assert!(rendered.contains("FunctionDefinition@"), "{rendered}");
}

#[test]
fn unsupported_typed_operation_reports_deterministic_span_bearing_missing_rule() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Main(i32 outer) { let task = spawn ((i32 inner) => outer + inner); return outer; }",
    );
    let spawn =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::SpawnExpression).expect("spawn expression");

    let error = emit_isle_expression(&input, isa.as_ref(), spawn, types::I64)
        .expect_err("unsupported spawn must not route around generated ISLE");
    let first = error.display_with_db(input.database());
    let repeated = error.display_with_db(input.database());

    assert_eq!(first, repeated);
    assert!(first.contains("MissingRuleOrFact"), "{first}");
    assert!(first.contains("SpawnExpression@"), "{first}");
}

#[test]
fn lambda_is_classified_for_generation_safe_isle_lowering() {
    assert_eq!(
        beskid_isle::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::LambdaExpression),
        beskid_isle::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::LambdaExpression),
    );
}

#[test]
fn unsupported_code_string_reports_deterministic_span_bearing_missing_rule() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main() { code ```beskid\nlet generated = 1;\n```; return 0; }");
    let code_string = find_node(input.database(), root, beskid_queries::IndexedNodeKind::CodeStringLiteral)
        .expect("code string literal");

    let error = emit_isle_expression(&input, isa.as_ref(), code_string, types::I64)
        .expect_err("code strings must not route around generated ISLE");
    let first = error.display_with_db(input.database());
    let repeated = error.display_with_db(input.database());

    assert_eq!(first, repeated);
    assert!(first.contains("MissingRuleOrFact"), "{first}");
    assert!(first.contains("CodeStringLiteral@"), "{first}");
}

/// CYB-106: every remaining UnsupportedTypedOperation host kind has a span-bearing
/// `MissingRuleOrFact` regression (construct `@` span, no HIR fallback).
#[test]
fn unsupported_host_composition_reports_deterministic_span_bearing_missing_rule() {
    const HOST_COMPOSITION_SOURCE: &str = r#"
host AppHost() {
    registry {
        single Logger;
    }
    scope Request() {
        single Logger;
    }
    startup() {
        return;
    }
}

type Logger {
    i32 value
}

i32 Main() {
    with Request() {
        return;
    }
    launch AppHost();
    return 0;
}
"#;
    for (source, kind, construct) in [
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::HostDefinition, "HostDefinition@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::RegistryBlock, "RegistryBlock@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::RegistryEntry, "RegistryEntry@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::ScopeDefinition, "ScopeDefinition@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::ScopeHook, "ScopeHook@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::WithStatement, "WithStatement@"),
        (HOST_COMPOSITION_SOURCE, beskid_queries::IndexedNodeKind::LaunchStatement, "LaunchStatement@"),
    ] {
        assert_eq!(
            beskid_isle::syntax_types::classify_syntax_node_kind(kind),
            beskid_isle::syntax_types::SyntaxNodeClassification::UnsupportedTypedOperation,
            "{kind:?}"
        );

        let (input, isa, root) = item_fixture_with_root(source);
        let node = find_node(input.database(), root, kind).unwrap_or_else(|| panic!("expected syntax node {kind:?}"));

        let error = emit_isle_expression(&input, isa.as_ref(), node, types::I64)
            .expect_err("unsupported typed operations must not route around generated ISLE");
        let first = error.display_with_db(input.database());
        let repeated = error.display_with_db(input.database());

        assert_eq!(first, repeated, "{kind:?}");
        assert!(first.contains("MissingRuleOrFact"), "{kind:?}: {first}");
        assert!(first.contains(construct), "{kind:?}: expected construct {construct} in {first}");
    }
}

#[test]
fn cast_facts_are_independent_of_the_shared_literal_syntax_classification() {
    let (input, _isa, root) = item_fixture_with_root("unit Main() { i64 widenedLiteral = 1; }");
    let literal = find_node(input.database(), root, beskid_queries::IndexedNodeKind::Literal).expect("typed literal");

    assert_eq!(
        beskid_isle::syntax_types::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::Literal),
        beskid_isle::syntax_types::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::LiteralExpression,)
    );
    assert_eq!(
        beskid_queries::cast_intents(input.database(), literal).expect("cast-intent query"),
        Some(Arc::from([CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::I64,
        }]))
    );
}

#[test]
fn closure_captures_and_spawn_target_are_independent_semantic_facts() {
    let (input, _isa, root) = item_fixture_with_root(
        "i32 Main(i32 outer) { let task = spawn ((i32 inner) => outer + inner); return outer; }",
    );
    let lambda = find_node(input.database(), root, beskid_queries::IndexedNodeKind::LambdaExpression)
        .expect("lambda expression");
    let spawn =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::SpawnExpression).expect("spawn expression");
    let closure = closure_environment(input.database(), lambda).expect("closure query").expect("closure facts");
    let target = spawn_target(input.database(), spawn).expect("spawn query").expect("spawn facts");
    let function = find_function_definition(input.database(), root).expect("function definition");

    assert_eq!(
        beskid_isle::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::LambdaExpression,),
        beskid_isle::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::LambdaExpression,),
    );
    assert_eq!(
        beskid_isle::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::SpawnExpression,),
        beskid_isle::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::SpawnExpression,),
    );
    assert_eq!(closure.parameters.len(), 1, "lambda parameter fact");
    assert_eq!(closure.captures.len(), 1, "outer capture fact");
    assert_eq!(
        node_kind(input.database(), closure.captures[0].declaration).expect("capture kind query"),
        Some(beskid_queries::IndexedNodeKind::Identifier),
    );
    assert_eq!(closure.captures[0].slot.owner, function);
    assert_eq!(closure.captures[0].slot.index, 0);
    assert_eq!(target.callee, lambda);
    assert_eq!(target.captures, closure.captures);
}
