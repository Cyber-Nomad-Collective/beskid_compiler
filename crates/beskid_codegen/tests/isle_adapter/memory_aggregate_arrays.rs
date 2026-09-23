use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, TargetMetadata, aggregate_field_access, build_typed_program, emit_isle_expression,
    emit_isle_item, empty_array_literal_element_abi_type, find_function_definition, find_node, find_nodes_of_kind, isa,
    item_fixture_with_root, parse_program_with_source_name, settings,
};
use super::support::{
    HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, call_lowering, default_libcall_names,
    emit_isle_item_with_call_importer, find_call_expression, find_function_definitions, function_signature,
};
use beskid_queries::contextual_integer_literal_abi_type;

#[test]
fn primitive_conversion_array_literal_preserves_wrapper_type_and_lowers() {
    use beskid_queries::{IndexedNodeKind, SemanticTypeId, child_nodes, node_type};

    let (input, isa, root) = item_fixture_with_root("word[] Main() { word[] result = [word(0)]; return result; }");
    let item = find_function_definition(input.database(), root).expect("Main");
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("primitive conversion array elements lower through real ISLE");
    let array = find_node(input.database(), item, IndexedNodeKind::ArrayLiteralExpression).expect("array");
    let wrapper = child_nodes(input.database(), array).unwrap().unwrap()[0];
    let call = find_node(input.database(), wrapper, IndexedNodeKind::CallExpression).expect("conversion");
    for key in [wrapper, call] {
        assert_eq!(node_type(input.database(), key).unwrap(), Some(SemanticTypeId::WORD));
    }
    let plan = input.array_static_plan(array).expect("typed array plan");
    assert_eq!(plan.element_type, SemanticTypeId::WORD);
    assert_eq!(plan.stride, u64::from(isa.pointer_type().bytes()));
    assert!(plan.pointer_map_offsets.is_empty(), "words are untraced integers, not pointers");
    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(clif.contains("store"), "{clif}");
}

#[test]
fn primitive_conversion_array_literal_rejects_invalid_conversions() {
    for expression in ["word(true)", "word(native)", "word(Unknown())", "word(0, 1)"] {
        let source = format!("word[] Main(pointer native) {{ return [{expression}]; }}");
        let (input, isa, root) = item_fixture_with_root(&source);
        let item = find_function_definition(input.database(), root).expect("Main");
        let array = find_node(input.database(), item, beskid_queries::IndexedNodeKind::ArrayLiteralExpression).unwrap();
        assert!(input.array_static_plan(array).is_none(), "invalid conversion cannot authorize storage: {expression}");
        let error = emit_isle_item(&input, isa.as_ref(), item).expect_err("invalid conversion must remain unavailable");
        assert!(error.display_with_db(input.database()).contains("MissingRuleOrFact"), "{expression}: {error:?}");
    }
}

#[test]
fn applied_generic_aggregate_plans_distinguish_pointer_and_scalar_fields() {
    let source = "type Applied<T> { T value, i64 rest } i64 Main() { Applied<string> pointerValue = Applied<string> { value: \"ok\", rest: 1_i64 }; Applied<i32> scalarValue = Applied<i32> { value: 2, rest: 3_i64 }; return scalarValue.rest; }";
    let (input, _isa, root) = item_fixture_with_root(source);
    let literals = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::StructLiteralExpression);
    let [pointer_literal, scalar_literal] = literals.as_slice() else {
        panic!("expected pointer and scalar aggregate literals, got {literals:?}");
    };
    let pointer = input.aggregate_static_plan(*pointer_literal).expect("pointer-applied aggregate plan");
    let scalar = input.aggregate_static_plan(*scalar_literal).expect("scalar-applied aggregate plan");

    assert_eq!(pointer.fields[0].abi_type, beskid_queries::SemanticTypeId::STRING);
    assert_eq!(pointer.pointer_map_offsets.as_ref(), &[pointer.fields[0].field_offset]);
    assert_eq!(scalar.fields[0].abi_type, beskid_queries::SemanticTypeId::I32);
    assert!(scalar.pointer_map_offsets.is_empty());
}

#[test]
fn applied_generic_integer_field_uses_the_instantiated_layout_when_lowered() {
    let (input, isa, root) = item_fixture_with_root(
        "type Applied<T> { T value } i32 Main() { Applied<i32> value = Applied<i32> { value: 7 }; return value.value; }",
    );
    let function = find_function_definition(input.database(), root).expect("Main definition");
    let literal = find_node(input.database(), root, beskid_queries::IndexedNodeKind::LiteralExpression)
        .expect("aggregate field integer literal");

    assert_eq!(
        contextual_integer_literal_abi_type(input.database(), literal).expect("contextual ABI query"),
        Some(beskid_queries::SemanticTypeId::I32),
        "the applied aggregate layout, rather than the generic declaration syntax, owns the field ABI"
    );

    let clif = emit_isle_item(&input, isa.as_ref(), function)
        .expect("the unsuffixed integer must inherit i32 from Applied<i32>.value")
        .display()
        .to_string();

    assert!(clif.contains("iconst.i32 7"), "the applied generic field must materialize at i32 width: {clif}");
    assert!(clif.contains("load.i32"), "the applied generic projection must retain the same i32 layout: {clif}");
}

#[test]
fn named_aggregate_fields_lower_in_declaration_layout_order() {
    let (input, isa, root) = item_fixture_with_root(
        "type Mixed { i32 narrow, i64 wide } i64 Main() { Mixed value = Mixed { wide: 9_i64, narrow: 7 }; return value.wide; }",
    );
    let function = find_function_definition(input.database(), root).expect("Main definition");
    let literal = find_node(input.database(), root, beskid_queries::IndexedNodeKind::StructLiteralExpression)
        .expect("aggregate literal");
    let plan = input.aggregate_static_plan(literal).expect("aggregate plan");
    let [narrow_layout, wide_layout] = plan.fields.as_ref() else {
        panic!("expected two physical fields: {plan:?}");
    };

    let function = emit_isle_item(&input, isa.as_ref(), function)
        .expect("named fields may be written in a different order from their declaration");
    let clif = function.display().to_string();
    use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef, types};
    let definition = |value| {
        let ValueDef::Result(inst, _) = function.dfg.value_def(value) else { panic!("instruction result: {clif}") };
        &function.dfg.insts[inst]
    };
    for (expected, ty, layout) in [(7, types::I32, narrow_layout), (9, types::I64, wide_layout)] {
        let stores = function
            .layout
            .blocks()
            .flat_map(|block| function.layout.block_insts(block))
            .filter_map(|inst| {
                let InstructionData::Store { args, offset, .. } = function.dfg.insts[inst] else { return None };
                let InstructionData::UnaryImm { opcode: Opcode::Iconst, imm } = definition(args[0]) else {
                    return None;
                };
                (i64::from(*imm) == expected && function.dfg.value_type(args[0]) == ty).then_some((args[1], offset))
            })
            .collect::<Vec<_>>();
        assert_eq!(stores.len(), 1, "field value must be stored exactly once: {clif}");
        let (address, offset) = stores[0];
        assert_eq!(i32::from(offset), 0);
        let InstructionData::Binary { opcode: Opcode::Iadd, args } = definition(address) else {
            panic!("field address: {clif}")
        };
        let InstructionData::UnaryImm { opcode: Opcode::Iconst, imm } = definition(args[1]) else {
            panic!("field offset: {clif}")
        };
        assert_eq!(
            u64::try_from(i64::from(*imm)).expect("positive field offset"),
            layout.field_offset,
            "field uses its declared offset: {clif}"
        );
    }
}

#[test]
fn parsed_struct_literal_uses_source_aggregate_layout_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { let point = Point { x: 1, y: 2 }; return 0; } type Point { i32 x, i32 y }";
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
    let literal =
        find_node(&db, root, beskid_queries::IndexedNodeKind::StructLiteralExpression).expect("struct literal");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    assert!(input.aggregate_static_plan(literal).is_some(), "aggregate static plan");
    let function = emit_isle_expression(&input, isa.as_ref(), literal, isa.pointer_type())
        .expect("aggregate literal lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(
        clif.contains("beskid_rt_v5_managed_object_allocate"),
        "aggregate literals must allocate through the canonical managed-object ABI: {clif}"
    );
    assert!(
        function.sized_stack_slots.is_empty(),
        "aggregate literals must not allocate escaping stack storage: {clif}"
    );
}

#[test]
fn parsed_empty_array_field_uses_declared_nominal_element_abi_without_hir() {
    let source = "type SyntaxContributionItem {} type GeneratedSyntaxContribution { SyntaxContributionItem[] items } GeneratedSyntaxContribution Main() { return GeneratedSyntaxContribution { items: [] }; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let array = find_node(input.database(), root, beskid_queries::IndexedNodeKind::ArrayLiteralExpression)
        .expect("empty array literal");
    assert_eq!(
        empty_array_literal_element_abi_type(input.database(), array).expect("empty array field fact"),
        Some(beskid_queries::SemanticTypeId::POINTER),
        "the nominal aggregate field, not a default machine type, authorizes the empty array element ABI"
    );
    assert!(input.array_static_plan(array).is_some(), "empty array has source-authorized static metadata");

    let function = find_function_definition(input.database(), root).expect("Main definition");
    let clif = emit_isle_item(&input, isa.as_ref(), function)
        .expect("declared empty aggregate-field array lowers through generated ISLE")
        .display()
        .to_string();
    assert!(
        clif.contains("beskid_rt_v5_array_allocate_rooted"),
        "empty array allocation must retain its descriptor-backed construction root: {clif}"
    );
    assert!(
        clif.contains("beskid_rt_v5_managed_object_allocate"),
        "the enclosing nominal aggregate remains a managed object: {clif}"
    );
}

#[test]
fn parsed_empty_array_local_uses_its_direct_declared_element_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "u8[] Main() { mut u8[] output = []; return output; }",
    );
    let array = find_node(input.database(), root, beskid_queries::IndexedNodeKind::ArrayLiteralExpression)
        .expect("empty array literal");

    assert_eq!(
        empty_array_literal_element_abi_type(input.database(), array).expect("empty array local fact"),
        Some(beskid_queries::SemanticTypeId::U8),
        "only the direct explicit local annotation supplies the empty array element ABI"
    );
    assert!(input.array_static_plan(array).is_some(), "empty local array has source-authorized static metadata");

    let function = find_function_definition(input.database(), root).expect("Main definition");
    let clif = emit_isle_item(&input, isa.as_ref(), function)
        .expect("declared empty local array lowers through generated ISLE")
        .display()
        .to_string();
    assert!(
        clif.contains("beskid_rt_v5_array_allocate_rooted"),
        "empty array allocation must retain its descriptor-backed construction root: {clif}"
    );
}

#[test]
fn empty_array_local_context_rejects_inferred_assignment_and_nested_literals() {
    for source in ["unit Main() { let values = []; }", "u8[] Main() { u8[] values = { [] }; return values; }"] {
        let (input, _isa, root) = item_fixture_with_root(source);
        let arrays = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::ArrayLiteralExpression);
        assert_eq!(arrays.len(), 1, "fixture has one empty literal: {source}");
        assert!(input.array_static_plan(arrays[0]).is_none(), "unproven empty literal context must stay unavailable: {source}");
    }

    let (input, _isa, root) = item_fixture_with_root("unit Main() { mut u8[] values = []; values = []; }");
    let arrays = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 2, "fixture has initializer and assignment literals");
    assert!(input.array_static_plan(arrays[0]).is_some(), "direct explicit initializer stays authorized");
    assert!(input.array_static_plan(arrays[1]).is_none(), "an assignment is not an initialization context");
}

#[test]
fn managed_struct_field_access_uses_allocation_plan_offsets() {
    // A managed aggregate is allocated behind a BeskidObjectHeader, so every field offset is
    // header-relative. Field access previously recomputed offsets from zero and therefore read the
    // header instead of the payload, which corrupted the loaded value (an enum tag read this way
    // reaches an exhaustive-match default trap and aborts with SIGILL at run time).
    let source = "type Point { i32 x, i32 y } i32 Main() { Point point = Point { x: 1, y: 2 }; return point.y; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let literal =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::StructLiteralExpression).expect("literal");
    let declaration =
        beskid_queries::aggregate_literal_declaration(input.database(), literal).expect("query").expect("declaration");
    let plan = input.aggregate_static_plan(literal).expect("aggregate static plan");
    let layout = input.aggregate_object_layout(declaration).expect("aggregate object layout");

    let header = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidObjectHeader")
        .expect("object header layout");
    assert_eq!(layout.fields.as_ref(), plan.fields.as_ref(), "construction and field access must share one layout");
    assert_eq!(layout.object_size, plan.object_size);
    assert_eq!(layout.object_alignment, plan.object_alignment);
    assert!(
        layout.fields.iter().all(|field| field.field_offset >= header.size),
        "managed field offsets must clear the object header: {layout:?} header={header:?}"
    );

    let function = find_function_definition(input.database(), root).expect("Main definition in fixture assembly");
    let clif = emit_isle_item(&input, isa.as_ref(), function).expect("field access lowers").display().to_string();
    let y_offset = layout.fields.last().expect("second field").field_offset;
    assert!(
        clif.contains(&format!("+{y_offset}")),
        "field read must address the offset the allocation reserved (+{y_offset}): {clif}"
    );
}

#[test]
fn parsed_nominal_parameter_field_read_lowers_without_hir() {
    let (input, isa, root) =
        item_fixture_with_root("type Style { i64 code } bool Main(Style chain) { return chain.code == 0; }");
    let item = find_function_definition(input.database(), root).expect("main item");
    let field =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::PathExpression).expect("field expression");
    assert!(
        aggregate_field_access(input.database(), field).expect("field query").is_some(),
        "field access syntax fact"
    );

    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("nominal parameter field read lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("load.i64"), "{clif}");
}

/// Lower `Main` after proving every index node in it addresses a managed nominal element by
/// reference (`POINTER`), never as an unresolved enclosing generic parameter.
fn lower_struct_array_main(source: &str) -> String {
    let (input, isa, root) = item_fixture_with_root(source);
    let indexes = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::IndexExpression);
    assert!(!indexes.is_empty(), "fixture indexes an array: {source}");
    for index in indexes {
        assert_eq!(
            beskid_queries::array_index_element_abi_type(input.database(), index).expect("element fact"),
            Some(beskid_queries::SemanticTypeId::POINTER),
            "a single-segment nominal element is concrete, not an enclosing generic parameter: {source}"
        );
    }
    let function = find_function_definition(input.database(), root).expect("Main definition");
    emit_isle_item(&input, isa.as_ref(), function)
        .unwrap_or_else(|error| panic!("struct array element access must lower: {error:?}\n{source}"))
        .display()
        .to_string()
}

#[test]
fn struct_array_element_read_into_local_lowers() {
    for source in [
        "type Chunk { i64 a, i64 b } i64 Main(Chunk[] batch, i64 i) { Chunk survivor = batch[i]; return survivor.b; }",
        "type Chunk { i64 a, i64 b } i64 Main(i64 i) { Chunk[] batch = [Chunk { a: 1, b: 2 }]; Chunk survivor = batch[i]; return survivor.b; }",
    ] {
        let clif = lower_struct_array_main(source);
        assert!(clif.contains("load.i64"), "managed element reference and its field are loaded: {clif}");
        assert!(clif.contains("heap_oob"), "element reads stay bounds-checked: {clif}");
    }
}

#[test]
fn inferred_local_field_read_lowers() {
    // An unannotated local takes its nominal type from its initializer's source-proven identity.
    let clif = lower_struct_array_main(
        "type Chunk { i64 a, i64 b } i64 Main(Chunk[] batch, i64 i) { let survivor = batch[i]; return survivor.b; }",
    );
    assert!(clif.contains("load.i64"), "inferred element local field is loaded: {clif}");
    assert!(clif.contains("heap_oob"), "element reads stay bounds-checked: {clif}");

    let source = "type Chunk { i64 a, i64 b } i64 Main() { let c = Chunk { a: 1, b: 2 }; return c.b; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let function = find_function_definition(input.database(), root).expect("Main definition");
    let clif = emit_isle_item(&input, isa.as_ref(), function)
        .unwrap_or_else(|error| panic!("inferred struct-literal local field read must lower: {error:?}"))
        .display()
        .to_string();
    assert!(clif.contains("load.i64"), "inferred struct-literal local field is loaded: {clif}");
}

#[test]
fn inferred_call_result_local_field_read_lowers() {
    let (input, isa, root) = item_fixture_with_root(
        "type Chunk { i64 a, i64 b } Chunk MakeChunk() { return Chunk { a: 1, b: 2 }; } i64 Main() { let c = MakeChunk(); return c.b; }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let [_, caller] = items.as_slice() else { panic!("MakeChunk and Main definitions: {items:?}") };
    let call = find_call_expression(db, *caller).expect("MakeChunk call");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(db, call).expect("direct-call query").expect("direct call")
    else {
        panic!("expected a syntax-resolved direct call");
    };
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), isa.pointer_type(), []);
    let imported = module.declare_function("MakeChunk", Linkage::Import, &signature).expect("declare MakeChunk");
    let mut importer =
        ItemModuleImporter::new(&mut module, HashMap::from([(beskid_isle::DirectCallee::item(declaration), imported)]));
    let clif = emit_isle_item_with_call_importer(&input, isa.as_ref(), *caller, &mut importer)
        .unwrap_or_else(|error| panic!("inferred call-result local field read must lower: {error:?}"))
        .display()
        .to_string();
    assert!(clif.contains("call"), "{clif}");
    assert!(clif.contains("load.i64"), "inferred call-result local field is loaded: {clif}");
}

#[test]
fn struct_array_element_write_from_local_lowers() {
    for source in [
        "type Chunk { i64 a, i64 b } unit Main(Chunk[] batch, i64 i, Chunk survivor) { batch[i] = survivor; }",
        "type Chunk { i64 a, i64 b } unit Main(Chunk[] batch, i64 i) { Chunk survivor = batch[0]; batch[i] = survivor; }",
    ] {
        let clif = lower_struct_array_main(source);
        assert!(clif.contains("store"), "managed element reference is stored: {clif}");
        assert!(clif.contains("heap_oob"), "element writes stay bounds-checked: {clif}");
    }
}
