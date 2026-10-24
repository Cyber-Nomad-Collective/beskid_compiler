use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CastIntent, CodegenInput,
    EffectiveCompilationRoots, FunctionEmitter, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit,
    SourceUnitId, SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, UserFuncName, build_typed_program,
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
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            origin_path: source_path.clone(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
    // A verification failure with a site is the internal error E2103 at that construct.
    assert!(first.contains("syntax ISLE emission failed: E2103 internal compiler error: "), "{first}");
    assert!(first.contains("FunctionDefinition@"), "{first}");
    let report = error.into_report(&input, "syntax ISLE lowering failed");
    let bundle = report
        .downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>()
        .expect("the internal error keeps its structured diagnostic");
    let diagnostic = &bundle.diagnostics()[0];
    assert_eq!(diagnostic.code.as_deref(), Some("E2103"));
    assert!(diagnostic.message.ends_with("please report this"), "{}", diagnostic.message);
    assert!(!diagnostic.message.contains("Verification("), "{}", diagnostic.message);
}

/// The ICE floor: a failure without any construct (here, two items claiming one symbol) still
/// renders as one coded internal diagnostic, anchored at the program, never as raw text.
#[test]
fn an_unsited_emission_failure_is_a_coded_internal_diagnostic() {
    let (input, _isa, _root) = item_fixture_with_root("i32 First() { return 1; }");
    let error = beskid_codegen::module_emission::SyntaxModuleEmissionError::DuplicateSymbol("Same".into());
    let report = error.into_report(&input, "syntax ISLE lowering failed");
    let bundle = report
        .downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>()
        .expect("an unsited failure still renders as a structured diagnostic");
    let diagnostic = &bundle.diagnostics()[0];
    assert_eq!(diagnostic.code.as_deref(), Some("E2105"));
    assert!(diagnostic.message.starts_with("internal compiler error: "), "{}", diagnostic.message);
    assert!(diagnostic.help.as_deref().is_some_and(|help| help.contains("duplicate symbol `Same`")));
    assert_eq!(diagnostic.span.offset(), 0, "anchored at the program");
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
/// `MissingRuleOrFact` regression for the construct span.
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
    i32 value;
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

/// Reassigning an immutable local must stop at the legality gate with E1214. Before the gate
/// carried `immutable_local_assignment`, the write reached ISLE `emit_local_assign`, whose
/// `mutable_local_assignment` fact fails closed, and surfaced as `MissingRuleOrFact
/// AssignExpression` (seen with a `pointer` local reassigned inside a `while` in a test body).
#[test]
fn immutable_local_reassignment_is_rejected_with_e1214_before_isle() {
    let function_cases = [
        "unit Main(pointer p) { pointer q = p; while true { q = p; } return; }",
        "unit Main() { i64 n = 0; while n < 3 { n = n + 1; } return; }",
        "pub type Pair { i64 a; } unit Main(bool c) { Pair v = Pair { a: 1 }; if c { v = Pair { a: 2 }; } return; }",
    ];
    for source in function_cases {
        let (input, isa, root) = item_fixture_with_root(source);
        let main = find_function_definitions(input.database(), root)[0];
        let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
            .expect_err("immutable reassignment must not lower");
        let rendered = error.to_string();
        assert!(rendered.contains("E1214"), "{source}: {rendered}");
        assert!(!rendered.contains("MissingRuleOrFact"), "{source}: {rendered}");
    }

    let (input, isa, root) =
        item_fixture_with_root("test sample { i64 n = 0; while n < 3 { n = n + 1; } }");
    let test = super::support::find_test_definition(input.database(), root).expect("test item");
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: test, symbol: "sample".into() }])
        .expect_err("immutable reassignment in a test body must not lower");
    let rendered = error.to_string();
    assert!(rendered.contains("E1214"), "{rendered}");
    assert!(!rendered.contains("MissingRuleOrFact"), "{rendered}");
}

/// A call whose callee names nothing must stop at the legality gate with a resolution code at
/// the call. Before the gate carried `unresolved_call_target`, `call_lowering` failed closed with
/// `unavailable("call_lowering")` and ISLE reported `MissingRuleOrFact CallExpression`.
#[test]
fn unknown_callee_is_rejected_with_a_resolution_code_before_isle() {
    let cases = [
        ("i64 Helper() { return 1; } i64 Main() { return Helpr(); }", 1, "E1101", "Helpr"),
        ("i64 Main() { return Missing.Helper(); }", 0, "E1108", "Missing"),
        ("T[] Empty<T>() { return []; } unit Main() { Empty(); return; }", 1, "E1203", ""),
    ];
    for (source, item, code, name) in cases {
        let (input, isa, root) = item_fixture_with_root(source);
        let db = input.database();
        let main = find_function_definitions(db, root)[item];
        let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
            .expect_err("an unknown callee must not lower");
        let rendered = error.to_string();
        assert!(rendered.contains(code), "{source}: {rendered}");
        assert!(rendered.contains(name), "{source}: {rendered}");
        assert!(!rendered.contains("MissingRuleOrFact"), "{source}: {rendered}");
    }
}

/// Unknown members, wrong enum payload counts and a match that leaves a variant without an arm
/// must stop at the legality gate with their own codes at the offending site, not reach ISLE as
/// an unavailable layout fact (`MissingRuleOrFact`) or `NonExhaustiveMatch`.
#[test]
fn member_and_match_violations_are_rejected_with_their_codes_before_isle() {
    let shape = "pub type Pair { i64 a; } enum Shape { Dot, Circle(i64 radius) } ";
    let cases = [
        ("i64 Main() { Pair p = Pair { b: 1 }; return 0; }", "E1211"),
        ("i64 Main(Pair p) { return p.b; }", "E1211"),
        ("i64 Main() { Shape s = Shape::Square; return 0; }", "E1301"),
        ("i64 Main() { Shape s = Shape::Circle(); return 0; }", "E1302"),
        ("i64 Main(Shape s) { return match s { Shape::Dot => 0, Shape::Circle(r, extra) => r, }; }", "E1307"),
        ("i64 Main(Shape s) { return match s { Shape::Dot => 0, }; }", "E1304"),
    ];
    for (body, code) in cases {
        let source = format!("{shape}{body}");
        let (input, isa, root) = item_fixture_with_root(&source);
        let main = find_function_definitions(input.database(), root)[0];
        let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
            .expect_err("the violation must not lower");
        let rendered = error.to_string();
        assert!(rendered.contains(code), "{source}: {rendered}");
        assert!(!rendered.contains("MissingRuleOrFact"), "{source}: {rendered}");
        assert!(!rendered.contains("NonExhaustiveMatch"), "{source}: {rendered}");
    }
}

/// A `use` of a module the assembly does not contain is dropped by import registration. The
/// legality gate rejects it as E1105 at the `use` for the unit of a lowered item, before any
/// ISLE rule selection.
#[test]
fn unknown_import_in_the_unit_of_a_lowered_item_is_rejected_as_e1105() {
    let (input, isa, root) = item_fixture_with_root("use Missing.Module;\ni64 Main() { return 0; }");
    let db = input.database();
    let main = find_function_definitions(db, root)[0];
    let findings = beskid_queries::check_items(db, &[main]).expect_err("an unknown import is rejected");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].kind.code(), "E1105");
    assert!(format_ast_node_site(db, findings[0].site).contains("UseDeclaration@"));
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect_err("an unknown import must not lower");
    let rendered = error.to_string();
    assert!(rendered.contains("E1105"), "{rendered}");
    assert!(!rendered.contains("MissingRuleOrFact"), "{rendered}");
}

/// Design slice 7: an invalid scoped `use` is judged by the legality gate for the items a
/// lowering request lowers (E1230), not while the typed program is admitted. An item nothing
/// requested may carry one without poisoning an unrelated request.
#[test]
fn invalid_scoped_use_is_rejected_as_e1230_only_when_its_item_is_lowered() {
    let source = "enum DisposeError { Failed(i64 code) } enum Result<T, E> { Ok(T value), Error(E error) } \
                  contract Disposable { Result<unit, DisposeError> Dispose(); } \
                  type Resource: Disposable { pub Result<unit, DisposeError> Dispose() { return Result::Ok(unit); } } \
                  Resource Open() { return Resource {}; } \
                  unit Bad() { use Resource resource = Open(); return; } \
                  i32 Main() { return 1; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let bad = functions[functions.len() - 2];
    let main = functions[functions.len() - 1];
    lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("an unrequested invalid scoped use must not poison lowering");
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: bad, symbol: "Bad".into() }])
        .expect_err("the invalid scoped use must not lower");
    let rendered = error.to_string();
    assert!(rendered.contains("E1230"), "{rendered}");
    assert!(rendered.contains("NonResultCallable"), "{rendered}");
    assert!(rendered.contains("ScopedUseStatement@"), "{rendered}");
    assert!(!rendered.contains("MissingRuleOrFact"), "{rendered}");
}

/// Design slice 2 (E1229): a generic parameter bound to two types by one call is rejected at the
/// call only when its item is lowered. An unrelated item lowers alone; requesting both reports
/// exactly one E1229 and no opaque specialization failure.
#[test]
fn generic_parameter_conflict_is_rejected_as_e1229_only_for_the_misusing_item() {
    let source = "i64 AsI64() { return 1_i64; } word AsWord() { return 1_word; } \
                  unit Equal<T>(T actual, T expected) { return; } \
                  unit Misuse() { Equal(AsI64(), AsWord()); return; } \
                  unit Fine() { Equal(AsI64(), 2_i64); return; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let item = |key, symbol: &str| SyntaxModuleItem { key, symbol: symbol.into() };
    let (as_i64, as_word, misuse, fine) = (functions[0], functions[1], functions[3], functions[4]);
    lower_syntax_program(&input, isa.as_ref(), &[item(fine, "Fine"), item(as_i64, "AsI64")])
        .expect("the consistent call lowers when requested alone");
    let error = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[item(misuse, "Misuse"), item(fine, "Fine"), item(as_i64, "AsI64"), item(as_word, "AsWord")],
    )
    .expect_err("the conflicting binding must not lower");
    let rendered = error.to_string();
    assert_eq!(rendered.matches("E1229").count(), 1, "{rendered}");
    assert!(rendered.contains("CallExpression@"), "{rendered}");
    assert!(!rendered.contains("generic specialization facts are unavailable"), "{rendered}");
    assert!(!rendered.contains("MissingRuleOrFact"), "{rendered}");
}

/// Design slice 8: after a clean legality gate, a construct no ISLE rule lowers is a compiler gap.
/// It is reported as internal error E2102 carrying the construct and its site, never with a
/// user-facing code, and converts into a source-excerpt diagnostic at the codegen boundary.
#[test]
fn missing_rule_after_a_clean_gate_is_internal_error_e2102_at_its_site() {
    let source = "i32 Main() { code ```beskid\nlet generated = 1;\n```; return 0; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = find_function_definitions(db, root)[0];
    beskid_queries::check_items(db, &[main]).expect("the legality gate finds no user error");
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect_err("an unported construct must not lower");
    let rendered = error.to_string();
    assert!(rendered.contains("E2102"), "{rendered}");
    assert!(rendered.contains("internal compiler error"), "{rendered}");
    // The unported code string surfaces at the statement that holds it.
    assert!(rendered.contains("ExpressionStatement@"), "{rendered}");
    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    let user_code = rendered.match_indices('E').any(|(index, _)| {
        let code = &rendered[index + 1..];
        code.len() >= 4 && code[..4].chars().all(|c| c.is_ascii_digit()) && !code.starts_with("21")
    });
    assert!(!user_code, "an internal error carries no user code: {rendered}");

    let report = error.into_report(&input, "syntax ISLE lowering failed");
    assert!(report.to_string().contains("E2102"), "{report}");
    let bundle = report
        .downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>()
        .expect("the internal error keeps its structured diagnostic");
    let diagnostic = &bundle.diagnostics()[0];
    assert_eq!(diagnostic.code.as_deref(), Some("E2102"));
    let excerpt = &source[diagnostic.span.offset()..diagnostic.span.offset() + diagnostic.span.len()];
    assert!(excerpt.starts_with("code ```beskid"), "the diagnostic points at the construct: `{excerpt}`");
}

/// Design slice 8: a legality rejection converts into one structured diagnostic per finding with
/// the offending unit's source excerpt, so `beskid test` FAIL lines and `beskid build` render
/// code, label, help and source instead of only `code message at site`.
#[test]
fn legality_rejection_converts_into_source_excerpt_diagnostics() {
    let source = "unit Main() { i64 n = 0; n = 1; i64 m = 0; m = 2; return; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let main = find_function_definitions(input.database(), root)[0];
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect_err("immutable reassignment must not lower");
    let report = error.into_report(&input, "syntax ISLE lowering failed");
    assert!(report.to_string().starts_with("syntax ISLE lowering failed: legality gate rejected"), "{report}");
    let bundle = report
        .downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>()
        .expect("legality findings keep their structured diagnostics");
    let diagnostic = &bundle.diagnostics()[0];
    assert_eq!(diagnostic.code.as_deref(), Some("E1214"));
    assert_eq!(bundle.diagnostics().len(), 2, "one diagnostic per immutable binding");
    assert_eq!(diagnostic.help.as_deref(), Some("write `mut i64 n = 0;`"));
    let excerpt = &source[diagnostic.span.offset()..diagnostic.span.offset() + diagnostic.span.len()];
    assert_eq!(excerpt, "n", "the diagnostic points at the declaration, where the fix goes");
    assert_eq!(diagnostic.span.offset(), source.find("n = 0").unwrap());
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].label, "assigned again");
    assert!(diagnostic.fixes[0].is_preferred(), "inserting `mut` is machine-applicable");
}

/// Owner report (beskid-lings): `total += 2` on a mutable local passed `beskid check` and then
/// failed lowering with E2102 `MissingRuleOrFact AssignExpression`. A compound assignment to a
/// mutable local lowers as `total = total op 2`, reading the local once.
#[test]
fn compound_assignment_to_a_mutable_local_lowers_for_every_operator() {
    for (operator, instruction) in [("+=", "iadd"), ("-=", "isub"), ("*=", "imul"), ("/=", "sdiv"), ("%=", "srem")] {
        let source = format!("i32 F() {{ mut i32 total = 1; total {operator} 2; return total; }}");
        let (input, isa, root) = item_fixture_with_root(&source);
        let function = find_function_definitions(input.database(), root)[0];
        beskid_queries::check_items(input.database(), &[function]).expect("a clean legality gate");
        let artifact =
            lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: function, symbol: "F".into() }])
                .unwrap_or_else(|error| panic!("`{operator}` must lower, never an internal error: {error}"));
        let clif = artifact.functions.iter().map(|lowered| lowered.function.display().to_string()).collect::<String>();
        assert!(clif.contains(instruction), "`{operator}` lowers to `{instruction}`:\n{clif}");
    }
}

/// A compound assignment to an immutable local is the user error E1214, never an internal error.
#[test]
fn compound_assignment_to_an_immutable_local_is_e1214() {
    for operator in ["+=", "-=", "*=", "/=", "%="] {
        let source = format!("i32 F() {{ i32 total = 1; total {operator} 2; return total; }}");
        let (input, isa, root) = item_fixture_with_root(&source);
        let function = find_function_definitions(input.database(), root)[0];
        let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: function, symbol: "F".into() }])
            .expect_err("an immutable target must not lower");
        let rendered = error.to_string();
        assert!(rendered.contains("E1214"), "{rendered}");
        assert!(!rendered.contains("E210"), "{rendered}");
    }
}

/// Lower every function of `source` (so calls between them resolve) and return the CLIF of the
/// last one, named `symbol`; the others are named `symbol_0`, `symbol_1`, ... in source order.
fn lower_one(source: &str, symbol: &str) -> Result<String, String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let functions = find_function_definitions(input.database(), root);
    assert!(!functions.is_empty(), "function");
    if let Err(findings) = beskid_queries::check_items(input.database(), &functions) {
        return Err(format!("{findings:?}"));
    }
    let items = functions
        .iter()
        .enumerate()
        .map(|(index, key)| SyntaxModuleItem {
            key: *key,
            symbol: if index + 1 == functions.len() { symbol.into() } else { format!("{symbol}_{index}").into() },
        })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items)
        .map(|artifact| {
            artifact
                .functions
                .iter()
                .find(|lowered| lowered.name == symbol)
                .map(|lowered| lowered.function.display().to_string())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())
}

/// Every numeric width keeps its own type through a compound assignment: `u8 += 1` stays `i8`
/// in CLIF (no silent failure on a widened operand), `u64 /= 2` divides unsigned, `f64 *= 2.0`
/// multiplies in floating point.
#[test]
fn compound_assignment_keeps_unsigned_and_float_widths() {
    let cases = [
        ("u8 F() { mut u8 level = 1; level += 1; level *= 3; level -= 1; level /= 2; level %= 2; return level; }", ["iadd", "udiv"]),
        ("u64 F() { mut u64 total = 10; total += 1; total /= 2; total %= 3; return total; }", ["iadd", "urem"]),
        ("f64 F() { mut f64 x = 1.5; x += 2.0; x *= 2.0; x -= 1.0; x /= 2.0; return x; }", ["fadd", "fdiv"]),
    ];
    for (source, instructions) in cases {
        let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}\n{source}"));
        for instruction in instructions {
            assert!(clif.contains(instruction), "`{instruction}` expected for {source}:\n{clif}");
        }
    }
}

/// `%=` on a float is the user error E1209 in analysis, never an internal error.
#[test]
fn float_remainder_assignment_is_a_user_error() {
    let source = "f64 F() { mut f64 x = 1.5; x %= 2.0; return x; }";
    let (input, _isa, root) = item_fixture_with_root(source);
    let function = find_function_definitions(input.database(), root)[0];
    let findings = beskid_queries::check_gate_obligations(input.database(), &[function]);
    let finding = findings.iter().find(|finding| finding.kind.code() == "E1209").expect("E1209 for f64 %=");
    assert_eq!(finding.kind.message(), "`%=` is not defined for `f64`");
}

/// `p.x op= v` and `xs[i] op= v` evaluate the receiver and the index once, read the place, and
/// store the combined value back.
#[test]
fn compound_assignment_to_fields_and_elements_lowers() {
    let field = "pub type Counter { i64 count; } \
                 i64 F(Counter counter) { counter.count += 2; counter.count *= 3; return counter.count; }";
    let clif = lower_one(field, "F").unwrap_or_else(|error| panic!("field target must lower: {error}"));
    assert!(clif.contains("iadd") && clif.contains("imul") && clif.contains("store"), "{clif}");
    let element = "i64 F(i64[] values) { values[0] += 5; values[1] -= 1; values[0] %= 4; return values[0]; }";
    let clif = lower_one(element, "F").unwrap_or_else(|error| panic!("element target must lower: {error}"));
    assert!(clif.contains("iadd") && clif.contains("isub") && clif.contains("srem"), "{clif}");
}

/// `string +=` on a local concatenates through the Corelib service.
#[test]
fn string_compound_addition_concatenates() {
    let source = "string F() { mut string text = \"a\"; text += \"b\"; return text; }";
    match lower_one(source, "F") {
        Ok(clif) => assert!(clif.contains("call"), "{clif}"),
        Err(error) => assert!(!error.contains("E210"), "string += must not be an internal error: {error}"),
    }
}

/// `cond ? a : b` lowers for every value representation: a `brif` on the condition, one block per
/// branch, and a merge block whose parameter carries the chosen value.
#[test]
fn conditional_expression_lowers_for_scalars_strings_and_nominal_types() {
    let cases = [
        ("i32 F(bool c, i32 a, i32 b) { return c ? a : b; }", "i32"),
        ("bool F(bool c, bool a, bool b) { return c ? a : b; }", "i8"),
        ("string F(bool c, string a, string b) { return c ? a : b; }", "i64"),
        ("f64 F(bool c, f64 a, f64 b) { return c ? a : b; }", "f64"),
        ("pub type Point { i64 x; } Point F(bool c, Point p1, Point p2) { Point p = c ? p1 : p2; return p; }", "i64"),
    ];
    for (source, merge_type) in cases {
        let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}\n{source}"));
        assert!(clif.contains("brif"), "`brif` expected for {source}:\n{clif}");
        assert!(
            has_single_parameter_block(&clif, merge_type),
            "merge block with one `{merge_type}` parameter expected for {source}:\n{clif}"
        );
    }
}

/// Whether the CLIF text has a block header with exactly one parameter of `ty`
/// (`block3(v5: i64):`), the shape of a conditional's merge block.
fn has_single_parameter_block(clif: &str, ty: &str) -> bool {
    clif.lines().map(str::trim).any(|line| {
        line.starts_with("block") && line.ends_with(&format!(": {ty}):")) && line.matches(':').count() == 2
    })
}

/// A conditional is an ordinary value: a `let` initializer, a call argument, a return value, and
/// a nested else-branch (`c ? 1 : d ? 2 : 3`, right-associative) all lower.
#[test]
fn conditional_expression_lowers_in_value_positions_and_nests() {
    let cases = [
        "i32 F(bool c, i32 a, i32 b) { i32 x = c ? a : b; return x + 1; }",
        "i64 G(i64 v) { return v; } i64 F(bool c, i64 a, i64 b) { return G(c ? a : b); }",
        "i32 F(bool c, bool d) { return c ? 1 : d ? 2 : 3; }",
        "i32 F(bool c, bool d, i32 a, i32 b, i32 e) { return c ? (d ? a : b) : e; }",
    ];
    for source in cases {
        let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}\n{source}"));
        assert!(clif.contains("brif"), "`brif` expected for {source}:\n{clif}");
    }
    let nested = "i32 F(bool c, bool d) { return c ? 1 : d ? 2 : 3; }";
    let clif = lower_one(nested, "F").expect("nested conditional lowers");
    assert_eq!(clif.matches("brif").count(), 2, "one `brif` per conditional:\n{clif}");
}

/// A bare numeric literal branch takes the other branch's type: `c ? 1 : 2_i64` is `i64` and the
/// `i32`-lowered literal is extended before the merge; `c ? n : 2` with `i64 n` likewise.
#[test]
fn conditional_expression_literal_branch_takes_the_other_branch_type() {
    let cases = [
        "i64 F(bool c) { return c ? 1 : 2_i64; }",
        "i64 F(bool c, i64 n) { return c ? n : 2; }",
        "i64 F(bool c, i64 n) { return c ? -1 : n; }",
        "u8 F(bool c, u8 n) { return c ? n : 2; }",
    ];
    for source in cases {
        let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}\n{source}"));
        assert!(clif.contains("brif"), "`brif` expected for {source}:\n{clif}");
        let merge_type = if source.starts_with("u8") { "i8" } else { "i64" };
        assert!(
            has_single_parameter_block(&clif, merge_type),
            "the merge carries the typed branch's width `{merge_type}` for {source}:\n{clif}"
        );
        assert!(!has_single_parameter_block(&clif, "i32"), "no `i32` merge for {source}:\n{clif}");
    }
}

/// Only the chosen branch is evaluated: two calls in the branches of one conditional sit in two
/// different blocks after the `brif`, never both in the entry block.
#[test]
fn conditional_expression_evaluates_only_the_chosen_branch() {
    let source = "i64 First() { return 1; } i64 Second() { return 2; } \
                  i64 F(bool c) { return c ? First() : Second(); }";
    let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}"));
    let blocks = clif.split("\nblock").skip(1).collect::<Vec<_>>();
    let calling_blocks = blocks.iter().filter(|block| block.contains("call ")).count();
    assert_eq!(calling_blocks, 2, "each call in its own block:\n{clif}");
    let branching = blocks.iter().find(|block| block.contains("brif")).expect("a block branches on the condition");
    assert!(!branching.contains("call "), "no call before the branch:\n{clif}");
}

/// A `unit`-valued conditional used as a statement (`c ? Left() : Right();`) evaluates exactly
/// one branch for its effects.
#[test]
fn conditional_expression_statement_of_unit_calls_lowers() {
    let source = "unit Left() { return; } unit Right() { return; } \
                  unit F(bool c) { c ? Left() : Right(); return; }";
    let clif = lower_one(source, "F").unwrap_or_else(|error| panic!("must lower: {error}"));
    assert!(clif.contains("brif"), "{clif}");
    assert_eq!(clif.matches("call ").count(), 2, "{clif}");
}

/// A well-formed conditional never leaves the gate with an internal error: the legality gate,
/// the gate obligations, and the typing obligations are all clean.
#[test]
fn conditional_expression_is_clean_under_every_check_pass() {
    let sources = [
        "i32 F(bool c, i32 a, i32 b) { return c ? a : b; }",
        "i64 F(bool c) { return c ? 1 : 2_i64; }",
        "pub type Point { i64 x; } Point F(bool c, Point p1, Point p2) { return c ? p1 : p2; }",
        "string F(bool c) { return c ? \"yes\" : \"no\"; }",
    ];
    for source in sources {
        let (input, _isa, root) = item_fixture_with_root(source);
        let function = *find_function_definitions(input.database(), root).last().expect("function");
        let db = input.database();
        if let Err(findings) = beskid_queries::check_items(db, &[function]) {
            panic!("check_items must be clean for {source}: {findings:?}");
        }
        let findings = beskid_queries::check_gate_obligations(db, &[function]);
        assert!(findings.is_empty(), "gate obligations must be clean for {source}: {findings:?}");
        let findings = beskid_queries::check_typing_obligations(db, &[function]);
        assert!(findings.is_empty(), "typing obligations must be clean for {source}: {findings:?}");
    }
}

/// The branch mismatch of a conditional renders with both branches as related labels.
#[test]
fn conditional_expression_branch_mismatch_renders_both_branches_as_related() {
    let source = "i64 F(bool c, i32 a, i64 b) { return c ? a : b; }";
    let (input, _isa, root) = item_fixture_with_root(source);
    let function = find_function_definitions(input.database(), root)[0];
    let findings = beskid_queries::check_gate_obligations(input.database(), &[function]);
    let finding = findings.iter().find(|finding| finding.kind.code() == "E1206").expect("E1206 for i32 against i64");
    let labels = finding.related.iter().map(|(_, label)| label.as_ref()).collect::<Vec<_>>();
    assert_eq!(labels, ["this branch is `i32`", "this branch is `i64`"]);
}
