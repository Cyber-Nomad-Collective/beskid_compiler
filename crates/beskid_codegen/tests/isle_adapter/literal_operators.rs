use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, NodeFacts, ProgramAssembly, ProjectSession, RootEntry, SourceUnit,
    SourceUnitId, SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, emit_isle_item,
    find_function_definition, find_function_definitions, find_node, find_test_definition, isa, item_fixture,
    item_fixture_with_root, lower_syntax_program, mutable_local_assignment, named_function, node_kind,
    parse_program_with_source_name, settings, test_statement_nodes,
};

#[test]
fn parsed_function_body_emits_verified_isle_clif_without_lowerable() {
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
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64").expect("host ISA").finish(flags).expect("host flags");

    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("parsed function body lowers through generated ISLE");

    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn parsed_u8_comparison_coerces_integer_literals_without_hir() {
    let (input, isa, item) = item_fixture("bool Main(u8 b) { return b > 57; }");

    let function = emit_isle_item(&input, isa.as_ref(), item).expect("u8 comparisons lower through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i8 57"), "{clif}");
}

#[test]
fn parsed_mixed_u8_i64_arithmetic_coerces_the_u8_operand_without_hir() {
    let (input, isa, item) = item_fixture("i64 Main(u8 b, i64 acc) { return acc + (b - 48); }");

    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("mixed-width arithmetic lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("uextend.i64"), "{clif}");
    assert!(clif.contains("iadd"), "{clif}");
}

#[test]
fn parsed_test_item_emits_verified_isle_clif_without_lowerable() {
    let (input, isa, root) = item_fixture_with_root("test Smoke { return; }");
    let item = find_test_definition(input.database(), root).expect("test item key");

    let statements =
        test_statement_nodes(input.database(), item).expect("test statement query").expect("test statement nodes");
    assert_eq!(statements.len(), 1);
    assert_eq!(
        node_kind(input.database(), statements[0]).expect("statement kind").expect("statement node"),
        beskid_queries::IndexedNodeKind::ReturnStatement
    );

    let function = emit_isle_item(&input, isa.as_ref(), item).expect("parsed test item lowers through generated ISLE");

    assert!(function.display().to_string().contains("return"));
}

#[test]
fn parsed_local_read_emits_verified_isle_clif_without_lowerable() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { i32 answer = 42; return answer; }";
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
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64").expect("host ISA").finish(flags).expect("host flags");

    let function = emit_isle_item(&input, isa.as_ref(), item).expect("parsed local read lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}

#[test]
fn parsed_parameter_read_materializes_the_generation_safe_local_slot() {
    let (input, isa, item) = item_fixture("i32 Identity(i32 value) { return value; }");

    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("parsed parameter read lowers through generated ISLE");
    let clif = function.display().to_string();
    assert!(clif.contains("function u0:0(i32) -> i32"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn parsed_mutable_range_accumulator_exposes_local_write_syntax_facts() {
    let (input, _isa, root) =
        item_fixture_with_root("i32 Main() { mut i32 sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }");
    let db = input.database();
    let assignment =
        find_node(db, root, beskid_queries::IndexedNodeKind::AssignExpression).expect("parsed accumulator assignment");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    let target = facts.child(assignment, 0).expect("assignment target fact");
    let declaration = beskid_queries::resolved_local(db, target)
        .expect("assignment target resolution")
        .expect("assignment target local")
        .declaration;
    let slot = beskid_queries::local_slot(db, declaration)
        .expect("assignment target slot")
        .expect("assignment target slot fact");
    assert_eq!(
        mutable_local_assignment(db, assignment).expect("mutable assignment query"),
        Some(beskid_queries::MutableLocalAssignment { declaration, slot })
    );
    assert_eq!(
        facts.mutable_local_assignment_slot(assignment),
        Some(beskid_isle::LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
    );
}

#[test]
fn parsed_mutable_string_local_exposes_local_write_syntax_facts() {
    let source = "string Main(bool enable) { mut string tail = \"h\"; if !enable { tail = \"l\"; } return tail; }";
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let assignment = find_node(db, root, beskid_queries::IndexedNodeKind::AssignExpression)
        .expect("parsed mutable string assignment");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    let target = facts.child(assignment, 0).expect("assignment target fact");
    let declaration = beskid_queries::resolved_local(db, target)
        .expect("assignment target resolution")
        .expect("assignment target local")
        .declaration;
    let slot = beskid_queries::local_slot(db, declaration)
        .expect("assignment target slot")
        .expect("assignment target slot fact");
    assert_eq!(
        mutable_local_assignment(db, assignment).expect("mutable assignment query"),
        Some(beskid_queries::MutableLocalAssignment { declaration, slot })
    );
    assert_eq!(
        facts.mutable_local_assignment_slot(assignment),
        Some(beskid_isle::LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
    );
}

#[test]
fn parsed_i64_local_initializers_and_assignments_contextualize_unsuffixed_integer_literals() {
    let (input, isa, root) =
        item_fixture_with_root("i64 Main(mut i64 start) { i64 offset = 0; start = 0; return start + offset; }");
    let item = named_function(&input, root, "Main");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("explicit i64 local initializer and assignment lower through syntax ISLE without widening");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 0"), "{clif}");
    assert!(!clif.contains("sextend"), "contextual literals must not become implicit numeric widening: {clif}");
}

#[test]
fn parsed_syntax_program_uses_the_existing_artifact_string_pool() {
    let (input, isa, root) = item_fixture_with_root("unit Main() { \"Beskid\"; return; }");
    let main = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("syntax item with a string literal lowers through the artifact pool");

    assert_eq!(artifact.string_literals.len(), 1);
    assert!(artifact.string_literals.values().any(|bytes| bytes.as_slice() == b"Beskid"));
}

#[test]
fn parsed_syntax_string_literal_materializes_runtime_string_abi() {
    let (input, isa, root) = item_fixture_with_root("string Main() { return \"ééé\"; }");
    let main = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("syntax string literal lowers through runtime ABI materialization");

    let clif = artifact.functions[0].function.display().to_string();
    assert!(clif.contains("str_new"), "syntax string literals must call the exact Corelib service: {clif}");
    assert!(clif.contains("iconst.i64 6"), "three UTF-8 e-acute scalars must materialize as six bytes: {clif}");
}
