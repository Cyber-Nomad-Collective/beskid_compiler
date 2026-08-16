use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, TargetMetadata, aggregate_field_access, build_typed_program, emit_isle_expression,
    emit_isle_item, empty_array_literal_element_abi_type, find_function_definition, find_node, isa,
    item_fixture_with_root, parse_program_with_source_name, settings,
};

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
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
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
    assert!(!clif.contains("stack_store"), "aggregate literals must not return escaped stack storage: {clif}");
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
