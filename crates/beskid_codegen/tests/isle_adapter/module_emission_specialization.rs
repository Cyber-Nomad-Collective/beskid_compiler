use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, JITBuilder, JITModule, Linkage, Module, ModuleIndex, NodeFacts, ProgramAssembly,
    ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem, TargetMetadata,
    build_typed_program, call_lowering, default_libcall_names, emit_syntax_program, find_call_expression,
    find_definition_of_kind, find_function_definition, find_function_definitions, find_node, find_test_definition, isa,
    item_fixture_with_root, item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

#[test]
fn ordinary_syscall_spelling_cannot_request_a_corelib_service_import() {
    let (input, _isa, root) = item_fixture_with_root("i64 Main() { return __syscall_write(1, \"application\"); }");
    let main = find_function_definition(input.database(), root).expect("application Main");
    let call = find_call_expression(input.database(), main).expect("application syscall spelling");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.direct_callee(call), None);
}

#[test]
fn parsed_program_declares_then_imports_syntax_items_without_hir() {
    let (input, isa, root) =
        item_fixture_with_root("i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }");
    let db = input.database();
    let items = find_function_definitions(db, root);
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "AddOne".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
        Linkage::Export,
    )
    .expect("syntax items declare before their direct-call bodies lower");
    assert_eq!(declared.len(), 2);
    assert_eq!(
        module.get_name("AddOne"),
        Some(cranelift_module::FuncOrDataId::Func(declared[&beskid_isle::DirectCallee::item(items[0])]))
    );
    assert_eq!(
        module.get_name("Main"),
        Some(cranelift_module::FuncOrDataId::Func(declared[&beskid_isle::DirectCallee::item(items[1])]))
    );
}

#[test]
fn parsed_program_lowers_to_backend_artifact_without_hir() {
    let (input, isa, root) =
        item_fixture_with_root("i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }");
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "AddOne".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("syntax items lower into a normal backend artifact");

    assert_eq!(artifact.functions.len(), 2);
    beskid_codegen::validate_artifact(&artifact).expect("direct syntax calls resolve against artifact definitions");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_syntax_program_omits_uncalled_generic_enum_declarations() {
    let (input, isa, root) = item_fixture_with_root(
        "type Box<T> { T value } enum Option<T> { Some(T value), None } i32 Main() { return 0; }",
    );
    let boxed = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::TypeDefinition)
        .expect("generic type declaration");
    let option = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumDefinition)
        .expect("generic enum declaration");
    let main = find_function_definitions(input.database(), root)[0];

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: boxed, symbol: "Box".into() },
            SyntaxModuleItem { key: option, symbol: "Option".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("generic declarations without executable bodies are omitted");

    assert_eq!(
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>(),
        ["Main"],
        "only executable syntax items enter the artifact",
    );
}

#[test]
fn parsed_struct_literal_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main() { return Point { x: 1 }.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method =
        find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("inline method source item");
    assert_eq!(
        beskid_isle::syntax_types::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::MethodDefinition),
        beskid_isle::syntax_types::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::MethodDefinition),
        "MethodDefinition must be production-supported at the ISLE inventory boundary"
    );
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(
        facts.node_kind(method),
        Some(beskid_isle::NodeKind::MethodDefinition),
        "adapter must surface MethodDefinition as an IsleLowered item kind"
    );
    let call = find_call_expression(db, main).expect("method call syntax");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(db, call).expect("method call query").expect("method call lowering")
    else {
        panic!("struct literal method call must resolve to its exact syntax declaration");
    };
    assert_eq!(declaration, method);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "Point_Ping".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("syntax-only module lowering supports the method receiver ABI");

    beskid_codegen::validate_artifact(&artifact).expect("method call imports the exact syntax method declaration");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_nominal_parameter_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main(Point point) { return point.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method =
        find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("inline method source item");
    let call = find_call_expression(db, main).expect("method call syntax");
    assert_eq!(call_lowering(db, call).expect("method call query"), Some(beskid_queries::CallLowering::Direct(method)));

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "Point_Ping".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("syntax-only module lowering supports an explicit nominal receiver ABI");

    beskid_codegen::validate_artifact(&artifact)
        .expect("nominal receiver call imports its exact syntax method declaration");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_program_specializes_an_inferred_generic_call_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } unit Main() { Equal(\"same\", \"same\", \"because\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Equal".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("syntax module specializes inferred generic calls through exact ABI facts");

    beskid_codegen::validate_artifact(&artifact).expect("the generic call imports its specialized item identity");
    assert_eq!(artifact.functions.len(), 2);
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "generic source items must use a mangled specialization identity"
    );
}

#[test]
fn parsed_program_emits_only_call_derived_generic_specializations_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Keep<T>(T value) { return 7; } i32 Unused<T>(T value) { return 0; } i32 Main() { return Keep(1); }",
    );
    let items = find_function_definitions(input.database(), root);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Keep".into() },
            SyntaxModuleItem { key: items[1], symbol: "Unused".into() },
            SyntaxModuleItem { key: items[2], symbol: "Main".into() },
        ],
    )
    .expect("only the generic declaration proven by an actual direct call is materialized");

    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Keep#generic_")),
        "the actual generic call must materialize its exact call-derived ABI specialization"
    );
    assert!(
        artifact.functions.iter().all(|function| !function.name.starts_with("Unused#generic_")),
        "a generic declaration without an actual direct call must not be materialized"
    );
}

#[test]
fn parsed_program_skips_uncalled_generic_template_bodies_without_an_environment() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Inner<T>(T value) { return; } unit Outer<T>(T value) { Inner<T>(value); return; } unit Main() { return; }",
    );
    let items = find_function_definitions(input.database(), root);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Inner".into() },
            SyntaxModuleItem { key: items[1], symbol: "Outer".into() },
            SyntaxModuleItem { key: items[2], symbol: "Main".into() },
        ],
    )
    .expect("uncalled generic templates are not executable roots");

    assert_eq!(artifact.functions.len(), 1);
    assert_eq!(artifact.functions[0].name, "Main");
}

#[test]
fn parsed_program_rejects_a_generic_direct_call_without_a_provable_specialization() {
    let (input, isa, root) = item_fixture_with_root("unit Missing<T>() { return; } unit Main() { Missing(); }");
    let items = find_function_definitions(input.database(), root);

    let error = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Missing".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect_err("a generic direct call without source-proven arguments must fail closed");

    let rendered = error.to_string();
    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("CallExpression@"), "{rendered}");
}

#[test]
fn parsed_program_specializes_generic_string_not_equal_as_content_comparison() {
    let (input, isa, root) = item_fixture_with_root(
        "unit NotEqual<T>(T actual, T expected) { if actual != expected { return; } return; } unit Main() { NotEqual(\"left\", \"right\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "NotEqual".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("generic string != lowers through its exact specialization");

    let not_equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("NotEqual#generic_"))
        .expect("specialized NotEqual<string> function");
    let clif = not_equal.function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "NotEqual<string> must dispatch through str_eq tag 42: {clif}");
    assert!(
        !clif.contains("icmp eq v0, v1") && !clif.contains("icmp ne v0, v1"),
        "NotEqual<string> must not compare raw string pointers: {clif}"
    );
}

#[test]
fn parsed_program_keeps_generic_nominal_pointer_equal_as_identity_comparison() {
    let (input, isa, root) = item_fixture_with_root(
        "type Box<T> { i64 value } unit Equal<T>(T actual, T expected) { if actual == expected { return; } return; } unit Main() { Box<i64> value = Box<i64> { value: 0_i64 }; Equal(value, value); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let equal = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Equal"))
        .expect("generic Equal function");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: equal, symbol: "Equal".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("generic nominal pointer equality lowers through its exact specialization");

    let equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Equal#generic_"))
        .expect("specialized Equal<Box<i64>> function");
    let clif = equal.function.display().to_string();
    assert!(!clif.contains("iconst.i32 42"), "nominal POINTER specialization must not dispatch through str_eq: {clif}");
    assert!(clif.contains("icmp eq v0, v1"), "nominal POINTER specialization must retain identity equality: {clif}");
}

#[test]
fn parsed_program_specializes_zero_argument_generic_factory_without_hir() {
    // Channel<T> Create<T>() collapses to POINTER at the ABI layer. Item ABI must still refuse a
    // fixed signature so module emission registers SpecializedItem, matching call-site imports.
    let (input, isa, root) = item_fixture_with_root(
        "type Channel<T> { i64 handle } Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; } unit Main() { Channel<i64> ch = Create<i64>(); return; }",
    );
    let items = find_function_definitions(input.database(), root);
    let create = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Create"))
        .expect("Create");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    assert_eq!(beskid_queries::item_abi_signature(input.database(), create).expect("generic item ABI"), None);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: create, symbol: "Create".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("zero-argument generic factories specialize through call-derived ABI identity");

    beskid_codegen::validate_artifact(&artifact)
        .expect("specialized factory imports must resolve against module declarations");
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Create#generic_")),
        "generic factory must emit a mangled specialization, not a bare Item identity"
    );
}

#[test]
fn parsed_program_specializes_a_generic_nominal_method_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type List<T> { T value, T Echo(T input) { return input; } } i64 Main(List<i64> list) { return list.Echo(1_i64); }",
    );
    let db = input.database();
    let method = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("List.Echo method");
    let main = find_function_definition(db, root).expect("Main item");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "List_Echo".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("explicit List<i64> receiver specializes its generic method");

    beskid_codegen::validate_artifact(&artifact).expect("specialized nominal method imports resolve");
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("List_Echo#generic_")),
        "generic owner methods must emit a call-derived specialization"
    );
}

#[test]
fn parsed_program_specializes_a_qualified_imported_generic_call_without_hir() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("project").keep();
    let main_path = directory.join("Main.bd");
    let assert_path = directory.join("Testing/Assert.bd");
    let main_source = "use Testing.Assert; enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } test Main { Result<string, string> result = Result::Ok(\"same\"); match result { Result::Ok(text) => { Assert.Equal(text, \"same\", \"because\"); }, Result::Error(_) => {}, }; }";
    let assert_source =
        "pub unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; }";
    std::fs::create_dir_all(assert_path.parent().expect("Testing directory")).expect("Testing directory");
    std::fs::write(&main_path, main_source).expect("main source");
    std::fs::write(&assert_path, assert_source).expect("assert source");
    let main_program = parse_program_with_source_name(main_path.to_str().unwrap(), main_source).expect("main parse");
    let assert_program =
        parse_program_with_source_name(assert_path.to_str().unwrap(), assert_source).expect("assert parse");
    let main_unit = SourceUnitId::new(&*db, main_path.clone());
    let assert_unit = SourceUnitId::new(&*db, assert_path.clone());
    let generation = SyntaxGenerationId(22);
    let project = ProjectSession::new(&*db, directory.clone(), main_path.clone(), "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main".into(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: "Testing.Assert".into(),
                path: assert_path,
                source: assert_source.into(),
                program: assert_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_root = AstNodeKey { unit: main_unit, generation, node: AstNodeId(0) };
    let assert_root = AstNodeKey { unit: assert_unit, generation, node: AstNodeId(0) };
    let generic = find_function_definition(&*db, assert_root).expect("generic function");
    let test = find_test_definition(&*db, main_root).expect("test item");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from([main_root, assert_root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: generic, symbol: "Equal".into() },
            SyntaxModuleItem { key: test, symbol: "Main".into() },
        ],
    )
    .expect("qualified generic calls produce exact syntax ABI specializations");

    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "qualified generic calls must emit their exact specialization",
    );
    let equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Equal#generic_"))
        .expect("specialized imported Assert.Equal function");
    let clif = equal.function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "Assert.Equal<string> must dispatch through str_eq tag 42: {clif}");
    assert!(!clif.contains("icmp eq"), "Assert.Equal<string> must not compare raw string pointers: {clif}");
}

#[test]
fn parsed_syntax_program_emits_imported_unit_calls_as_statements() {
    let (input, isa, root) = item_fixture_with_root("unit Assert() { } unit Main() { Assert(); }");
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Assert".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("syntax program with a unit call lowers through its statement rule");

    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main function");
    assert!(main.function.display().to_string().contains("call"));
}
