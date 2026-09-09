use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, HashMap, JITBuilder, JITModule, Linkage, Module, ModuleIndex, NodeFacts,
    ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem,
    TargetMetadata, build_typed_program, default_libcall_names, emit_isle_item, find_function_definition,
    find_function_definitions, find_node, find_nodes_of_kind, find_test_definition, isa, item_fixture,
    item_fixture_with_root, lower_syntax_program, mutable_local_assignment, named_function, node_kind,
    parse_program_with_source_name, settings, test_statement_nodes,
};

extern "C" fn test_str_new(bytes: *const u8, _byte_len: usize) -> *const u8 {
    bytes
}

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
fn parsed_f64_comparison_lowers_when_nested_in_a_boolean_call_argument() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Accept(bool value) { return; } unit Main() { f64 value = 0.5; Accept(value >= 0.0); return; }",
    );
    let functions = find_function_definitions(input.database(), root);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: functions[0], symbol: "Accept".into() },
            SyntaxModuleItem { key: functions[1], symbol: "Main".into() },
        ],
    )
    .expect("an f64 comparison used as a bool argument lowers through syntax facts");

    let main =
        artifact.functions.iter().find(|function| function.name.starts_with("Main")).expect("lowered Main function");
    assert!(main.function.display().to_string().contains("fcmp"));
}

#[test]
fn explicit_i64_to_f64_conversion_lowers_through_the_numeric_conversion_construct() {
    let (input, isa, item) = item_fixture(
        "f64 Main(mut i64 raw) { if raw < 0 { raw = -raw; } f64 result = f64(raw); return result / 9223372036854775808.0; }",
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("an explicit signed-integer to f64 conversion lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("fcvt_from_sint.f64"), "{clif}");
    assert!(clif.contains("fdiv"), "{clif}");
    assert!(!clif.contains("sdiv"), "floating-point division must not use the integer path: {clif}");
}

#[test]
fn parsed_short_circuit_comparison_materializes_a_declared_integer_constant() {
    let (input, isa, item) = item_fixture(
        "const CAPACITY = 64; bool Main(word slot, word count) { return slot >= count || slot >= CAPACITY; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("short-circuit comparison with a declared integer constant lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.matches("icmp").count() >= 2, "{clif}");
    assert!(clif.contains("iconst.i64 64"), "{clif}");
}

#[test]
fn parsed_mutable_assignment_materializes_a_declared_integer_constant_at_the_storage_abi() {
    let (input, isa, item) = item_fixture(
        "const DEFAULT_CAPACITY = 16; i64 Main(mut i64 capacity) { if capacity == 0 { capacity = DEFAULT_CAPACITY; } return capacity; }",
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("declared integer constant uses the mutable destination storage ABI");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 16"), "{clif}");
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
fn u32_boundary_zero_extends_and_relational_comparison_is_unsigned() {
    let (input, isa, item) = item_fixture("bool Main(u8 value) { u32 wide = value; return wide < 4294967295_u32; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("u32 storage boundary and comparison lower with unsigned semantics");
    let clif = function.display().to_string();
    assert!(clif.contains("uextend.i32"), "u8-to-u32 storage must zero extend:\n{clif}");
    assert!(clif.contains("icmp ult"), "u32 relational comparison must be unsigned:\n{clif}");
    assert!(!clif.contains("icmp slt"), "u32 must not alias signed i32 comparison semantics:\n{clif}");
}

#[test]
fn u32_signedness_is_symmetric_for_a_contextual_left_literal() {
    let (input, isa, item) = item_fixture("bool Main(u32 value) { return 1 < value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a left contextual literal inherits the right u32 operand semantics");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult"), "the right u32 operand must select unsigned comparison:\n{clif}");
    assert!(!clif.contains("icmp slt"), "operand order must not select signed comparison:\n{clif}");
}

#[test]
fn u32_division_and_remainder_use_unsigned_clif_operations() {
    let (input, isa, item) =
        item_fixture("u32 Main(u32 value) { u32 quotient = value / 2_u32; return quotient % 3_u32; }");

    let function = emit_isle_item(&input, isa.as_ref(), item).expect("u32 division and remainder lower");
    let clif = function.display().to_string();
    assert!(clif.contains("udiv"), "u32 division must be unsigned:\n{clif}");
    assert!(clif.contains("urem"), "u32 remainder must be unsigned:\n{clif}");
    assert!(!clif.contains("sdiv"), "u32 division must not alias i32:\n{clif}");
    assert!(!clif.contains("srem"), "u32 remainder must not alias i32:\n{clif}");
}

#[test]
fn grouped_nested_word_modulo_retains_unsigned_operand_authority() {
    let (input, isa, item) = item_fixture("word Main(word tail) { word nextTail = (tail + 1) % 32; return nextTail; }");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let outer = find_nodes_of_kind(input.database(), item, beskid_queries::IndexedNodeKind::BinaryExpression)
        .into_iter()
        .next()
        .expect("outer modulo expression");
    let grouped = facts.child(outer, 0).expect("grouped left operand");
    assert_eq!(facts.semantic_type(grouped), Some(beskid_queries::SemanticTypeId::WORD));

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a grouped nested word addition retains word authority for modulo lowering");
    let clif = function.display().to_string();
    assert!(clif.contains("urem"), "word modulo must use unsigned remainder:\n{clif}");
    assert!(!clif.contains("srem"), "word modulo must not use signed remainder:\n{clif}");
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
    let literal_globals = artifact.functions[0]
        .function
        .global_values
        .values()
        .filter_map(|global| match global {
            cranelift_codegen::ir::GlobalValueData::Symbol { colocated, .. } => Some(*colocated),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(literal_globals, vec![false], "JIT literal data may be more than 2 GiB from generated code");

    let mut builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
    builder.symbol("str_new", test_str_new as *const u8);
    let mut module = JITModule::new(builder);
    let mut function_ids = HashMap::new();
    beskid_codegen::cranelift_host::declare_user_functions(&mut module, &artifact, Linkage::Local, &mut function_ids)
        .expect("declare literal-producing function");
    let mut signature = module.make_signature();
    signature.params.push(cranelift_codegen::ir::AbiParam::new(isa.pointer_type()));
    signature.params.push(cranelift_codegen::ir::AbiParam::new(isa.pointer_type()));
    signature.returns.push(cranelift_codegen::ir::AbiParam::new(isa.pointer_type()));
    let str_new = module.declare_function("str_new", Linkage::Import, &signature).expect("declare str_new");
    function_ids.insert("str_new".into(), str_new);
    beskid_codegen::emit_string_literals(&mut module, &artifact).expect("emit literal data");

    let mut context = module.make_context();
    context.func = artifact.functions[0].function.clone();
    beskid_codegen::cranelift_host::remap_testcase_externals(&module, &mut context, &function_ids)
        .expect("remap literal data and str_new references");
    module.define_function(function_ids["Main"], &mut context).expect("define literal-producing function");
    module.finalize_definitions().expect("finalize literal-producing function and its data relocation");
}
