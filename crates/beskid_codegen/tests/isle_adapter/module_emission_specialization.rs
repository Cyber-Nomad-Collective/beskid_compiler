use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase,
    CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH, CodegenInput, EffectiveCompilationRoots, JITBuilder, JITModule, Linkage,
    Module, ModuleIndex, NodeFacts, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem, TargetMetadata, build_typed_program,
    build_typed_program_with_corelib_services, call_abi_signature, call_lowering, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_corelib_service_sources, default_libcall_names,
    emit_syntax_program, find_call_expression, find_corelib_service_call, find_definition_of_kind,
    find_function_definition, find_function_definitions, find_node, find_nodes_of_kind, find_test_definition, isa,
    item_fixture_with_root, item_name, lower_syntax_program, parse_program_with_source_name, settings,
    typed_array_allocation,
};
use cranelift_codegen::ir::types;

#[derive(Clone, Copy)]
enum StringComparison {
    Equal,
    NotEqual,
}

fn assert_string_content_comparison(clif: &str, comparison: StringComparison, scenario: &str) {
    let service_import = clif
        .lines()
        .find(|line| line.contains(" = %str_eq "))
        .unwrap_or_else(|| panic!("{scenario} must import the manifest-authorized str_eq service:\n{clif}"));
    let service_ref =
        service_import.split_whitespace().next().expect("str_eq import has a Cranelift function reference");
    let service_call = clif
        .lines()
        .find(|line| line.contains(&format!("= call {service_ref}(")))
        .unwrap_or_else(|| panic!("{scenario} must call its str_eq import:\n{clif}"));
    let service_result = service_call.split_whitespace().next().expect("str_eq call has a Cranelift result value");
    let zero = clif
        .lines()
        .find(|line| line.contains(" = iconst.i64 0"))
        .unwrap_or_else(|| panic!("{scenario} must compare the str_eq result with zero:\n{clif}"))
        .split_whitespace()
        .next()
        .expect("zero constant has a Cranelift value");
    let predicate = match comparison {
        StringComparison::Equal => "ne",
        StringComparison::NotEqual => "eq",
    };
    let expected = format!("icmp {predicate} {service_result}, {zero}");
    assert!(clif.contains(&expected), "{scenario} must derive its result from str_eq via `{expected}`:\n{clif}");
}

#[test]
fn nested_module_static_call_results_are_valid_comparison_operands() {
    let mut db = Box::new(BeskidDatabase::default());
    let project_root = tempfile::tempdir().expect("project").keep();
    let root = project_root.join("src");
    let main_path = root.join("Generated.bd");
    let cursor_path = root.join("Ansi/Cursor.bd");
    let sources = [
        (
            &main_path,
            "use Ansi.Cursor;\nbool Main(Ansi.Cursor.ParseResult<string> parsed) { Ansi.Cursor.CursorBuilder builder = Ansi.Cursor.Position(Ansi.Cursor.Start(), 1, 2); Ansi.Cursor.ParseResult<string> echoed = Ansi.Cursor.Echo(builder, parsed); return Ansi.Cursor.Width(builder) > Ansi.Cursor.Zero(); }",
        ),
        (
            &cursor_path,
            "pub type CursorBuilder { i64 row } pub enum ParseResult<T> { Ok(T value), Err } pub CursorBuilder Start() { return CursorBuilder { row: 0_i64 }; } pub CursorBuilder Position(CursorBuilder self, i64 row, i64 col) { return CursorBuilder { row: row }; } pub ParseResult<string> Echo(CursorBuilder cursor, ParseResult<string> parsed) { return parsed; } pub i64 Width(CursorBuilder self) { return self.row; } pub i64 Zero() { return 0_i64; }",
        ),
    ];
    for (path, source) in &sources {
        std::fs::create_dir_all(path.parent().expect("source parent")).expect("create source parent");
        std::fs::write(path, source).expect("write source");
    }
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 path"), source).expect("parse"),
        })
        .collect::<Vec<_>>();
    let generation = SyntaxGenerationId(140);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units.clone()),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&*db, main_path.clone());
    let cursor_unit = SourceUnitId::new(&*db, cursor_path);
    let project = ProjectSession::new(&*db, project_root, main_path, "nested-static".into(), "nested-static".into());
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&units[0].program, generation);
    let cursor_index = SyntaxIndex::from_program(&units[1].program, generation);
    let main = AstNodeKey {
        unit: main_unit,
        generation,
        node: main_index.ids_of_kind(beskid_queries::IndexedNodeKind::FunctionDefinition).next().expect("Main"),
    };
    let cursor_functions = cursor_index
        .ids_of_kind(beskid_queries::IndexedNodeKind::FunctionDefinition)
        .map(|node| AstNodeKey { unit: cursor_unit, generation, node })
        .collect::<Vec<_>>();
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let program = AstNodeKey { unit: main_unit, generation, node: AstNodeId(0) };
    let input = CodegenInput::new(leaked, typed, Arc::from([program]), target, manifest).expect("codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: cursor_functions[0], symbol: "Start".into() },
            SyntaxModuleItem { key: cursor_functions[1], symbol: "Position".into() },
            SyntaxModuleItem { key: cursor_functions[2], symbol: "Echo".into() },
            SyntaxModuleItem { key: cursor_functions[3], symbol: "Width".into() },
            SyntaxModuleItem { key: cursor_functions[4], symbol: "Zero".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("nested module static call results remain valid comparison operands");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact");
    assert!(main.function.display().to_string().contains("call"));
}

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
fn captured_char_lambda_uses_i32_in_its_environment_and_target_signature() {
    let (input, isa, root) = item_fixture_with_root("i32 Main(char outer) { return (() => outer)(); }");
    let main = find_function_definition(input.database(), root).expect("application Main");

    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("captured char lambda lowers through the canonical ABI mapping");

    let lambda = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_lambda_entry_syntax"))
        .expect("lambda trampoline target");
    assert_eq!(lambda.function.signature.params[0].value_type, types::I64, "closure environment is a pointer");
    assert!(
        lambda.function.display().to_string().contains("load.i32"),
        "a captured char must retain the canonical i32 ABI representation"
    );
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
fn nested_applied_generic_call_inside_an_aggregate_literal_lowers_from_one_environment() {
    let (input, isa, root) = item_fixture_with_root(
        "type Entry<TKey, TValue> { TKey key, TValue value } type Map<TKey, TValue> { Entry<TKey, TValue>[] entries, i64 count } T[] Identity<T>(T[] values) { return values; } Map<TKey, TValue> New<TKey, TValue>(Entry<TKey, TValue>[] empty) { return Map<TKey, TValue> { entries: Identity<Entry<TKey, TValue>>(empty), count: 0 }; } i64 Main(Entry<i64, string>[] empty) { Map<i64, string> map = New<i64, string>(empty); return map.count; }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("function name").to_string(),
        })
        .collect::<Vec<_>>();
    let new = items.iter().find(|item| item.symbol == "New").map(|item| item.key).expect("New function");
    let main = items.iter().find(|item| item.symbol == "Main").map(|item| item.key).expect("Main function");
    let call = find_call_expression(input.database(), main).expect("New call");
    let specialization = beskid_queries::generic_call_specialization(input.database(), call)
        .expect("New specialization query")
        .expect("New specialization");
    let instance = beskid_queries::generic_call_specialization_instance(input.database(), specialization)
        .expect("New instance query")
        .expect("New instance");
    let literal = find_node(input.database(), new, beskid_queries::IndexedNodeKind::StructLiteralExpression)
        .expect("Map literal");
    assert!(
        beskid_queries::aggregate_literal_specialization(input.database(), literal, instance.substitutions.clone(),)
            .expect("Map layout specialization query")
            .is_some(),
        "the enclosing source environment must materialize the Map layout",
    );
    assert!(
        input.aggregate_static_plan_for_specialization(literal, Some(&instance)).is_some(),
        "the specialized Map layout must produce one allocation plan",
    );
    let nested = find_call_expression(input.database(), new).expect("nested Identity<Entry<TKey, TValue>> call");
    assert!(
        beskid_queries::generic_call_specialization_in_environment(input.database(), nested, &instance)
            .expect("nested call environment query")
            .is_some(),
        "the nested call must consume the same recursive source environment",
    );

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("nested applied source arguments and their aggregate literal share one specialization environment");
}

#[test]
fn generic_aggregate_literal_keeps_declared_scalar_field_widths() {
    let (input, isa, root) = item_fixture_with_root(
        "type Queue<T> { T[] storage, i64 head, i64 count } T[] Identity<T>(T[] values) { return values; } Queue<T> New<T>(T[] empty) { return Queue<T> { storage: Identity<T>(empty), head: 0, count: 0 }; } i64 Main(string[] empty) { Queue<string> queue = New<string>(empty); return queue.count; }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("function name").to_string(),
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("generic aggregate specialization retains i64 fields beside pointer-shaped storage");
}

#[test]
fn unqualified_sibling_method_reuses_the_enclosing_nominal_specialization() {
    let (input, isa, root) = item_fixture_with_root(
        "type Set<T> { T[] values, bool Contains(T value) { return false; } Set<T> Add(T value) { if Contains(value) { return self; } return self; } } unit Main(Set<string> set) { set.Add(\"x\"); return; }",
    );
    let mut keys = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(find_function_definitions(input.database(), root));
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("an unqualified sibling method call consumes the current Set<string> environment");
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
fn generic_enum_constructor_uses_mutable_local_assignment_context() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Option<T> { Some(T value), None } Option<T> Last<T>(T value) { mut Option<T> last = Option::None(); last = Option::Some(value); return last; } Option<i64> Main() { return Last<i64>(7_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Last".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("a generic enum constructor assigned to a typed mutable local must retain its enclosing specialization");

    beskid_codegen::validate_artifact(&artifact).expect("generic enum assignment artifact");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Last#generic_")));
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
    assert_string_content_comparison(&clif, StringComparison::NotEqual, "NotEqual<string>");
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
    assert!(!clif.contains("%str_eq"), "nominal POINTER specialization must not call str_eq: {clif}");
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
fn parsed_program_specializes_generic_aggregate_literal_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Pair<T> { T value, i64 rest } Pair<T> Make<T>(T value) { return Pair<T> { value: value, rest: 9_i64 }; } i64 Main() { Pair<i32> pair = Make<i32>(7); return pair.rest; }",
    );
    let items = find_function_definitions(input.database(), root);
    let make = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Make"))
        .expect("Make");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: make, symbol: "Make".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("generic aggregate literals must lower through the enclosing item specialization");

    beskid_codegen::validate_artifact(&artifact)
        .expect("the specialized aggregate factory must resolve its data and call imports");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Make#generic_")));
}

#[test]
fn parsed_program_specializes_generic_pattern_payload_field_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Applied<T> { T value, i64 rest } enum Result<T> { Ok(Applied<T> success), Err(i64 error) } i64 Rest<T>(Result<T> result) { return match result { Result::Ok(success) => success.rest, Result::Err(_) => -1_i64, }; } i64 Main(Result<i32> result) { return Rest<i32>(result); }",
    );
    let items = find_function_definitions(input.database(), root);
    let rest = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Rest"))
        .expect("Rest");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: rest, symbol: "Rest".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("generic pattern payload projections must use the enclosing item specialization");

    beskid_codegen::validate_artifact(&artifact)
        .expect("the specialized generic match helper must resolve its call imports");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Rest#generic_")));
}

#[test]
fn canonical_generic_array_operations_preserve_concrete_types_through_nested_calls_and_imports() {
    let mut db = Box::new(BeskidDatabase::default());
    let application_root = tempfile::tempdir().expect("application project").keep();
    let application_path = application_root.join("Main.bd");
    let application_source = "use Core.Collections.Array; use Core.Collections.Array.ArrayIter; unit Check(bool condition, string because) { return; } i64 Main() { mut i64[] values = Array.Empty<i64>(); values = Array.Set<i64>(values, 0, 1); Check(Array.Capacity<i64>(values) >= Array.Len<i64>(values), \"capacity\"); mut ArrayIter<i64> iterator = Array.Iterate<i64>(values); return Array.Len<i64>(values); }";
    std::fs::write(&application_path, application_source).expect("write application source");

    let array_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH)
        .expect("canonical Array physical path");
    let foundation_root = array_path
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .expect("Foundation source root")
        .to_path_buf();
    let array_source = std::fs::read_to_string(&array_path).expect("canonical Array source");
    let array_iter_path = array_path.parent().expect("Array module parent").join("Array/ArrayIter.bd");
    let array_iter_source = std::fs::read_to_string(&array_iter_path).expect("canonical ArrayIter source");
    let embedded = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH)
        .expect("embedded Array source");
    assert_eq!(embedded.source.as_ref(), array_source);
    assert_eq!(
        canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH).as_deref(),
        Some(array_path.as_path())
    );
    let application_program = parse_program_with_source_name(application_path.to_str().unwrap(), application_source)
        .expect("parse application source");
    let array_program =
        parse_program_with_source_name(array_path.to_str().unwrap(), &array_source).expect("parse canonical Array");
    let array_iter_program = parse_program_with_source_name(array_iter_path.to_str().unwrap(), &array_iter_source)
        .expect("parse canonical ArrayIter");

    let application_unit = SourceUnitId::new(&*db, application_path.clone());
    let array_unit = SourceUnitId::new(&*db, array_path.clone());
    let project = ProjectSession::new(
        &*db,
        application_root.clone(),
        application_path.clone(),
        "App".into(),
        "typed-array-specialization".into(),
    );
    let generation = SyntaxGenerationId(104);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: application_root },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: foundation_root,
            }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main.bd".into(),
                path: application_path,
                source: application_source.into(),
                program: application_program,
            },
            SourceUnit {
                logical_name: "Core/Collections/Array.bd".into(),
                path: array_path,
                source: array_source,
                program: array_program,
            },
            SourceUnit {
                logical_name: "Core/Collections/Array/ArrayIter.bd".into(),
                path: array_iter_path,
                source: array_iter_source,
                program: array_iter_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib source authority"),
    )
    .expect("typed application and canonical Array source");
    let application_root = AstNodeKey { unit: application_unit, generation, node: AstNodeId(0) };
    let array_root = AstNodeKey { unit: array_unit, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([application_root]), target, manifest)
        .expect("generation-safe typed-array input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let empty = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Empty"))
        .expect("Array.Empty declaration");
    let len = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Len"))
        .expect("Array.Len declaration");
    let set = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Set"))
        .expect("Array.Set declaration");
    let iterate = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Iterate"))
        .expect("Array.Iterate declaration");
    let length_call =
        find_corelib_service_call(input.database(), len, "__array_len").expect("canonical Array.Len service call");
    assert!(matches!(
        call_lowering(input.database(), length_call).expect("Array.Len call lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__array_len" && service.symbol == "array_len"
    ));
    assert_eq!(
        call_abi_signature(input.database(), length_call).expect("Array.Len ABI signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([beskid_queries::SemanticTypeId::POINTER]),
            result: beskid_queries::SemanticTypeId::WORD,
        })
    );
    let allocation_call = find_call_expression(input.database(), empty).expect("Array.Empty allocation call");
    assert_eq!(
        typed_array_allocation(input.database(), allocation_call)
            .expect("typed allocation query")
            .expect("canonical Array allocation fact")
            .element_parameter
            .as_ref(),
        "T"
    );
    let application_items = find_function_definitions(input.database(), application_root);
    let check = application_items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Check"))
        .expect("application Check");
    let main = application_items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("application Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: empty, symbol: "Array_Empty".into() },
            SyntaxModuleItem { key: len, symbol: "Array_Len".into() },
            SyntaxModuleItem { key: set, symbol: "Array_Set".into() },
            SyntaxModuleItem { key: iterate, symbol: "Array_Iterate".into() },
            SyntaxModuleItem { key: check, symbol: "Check".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("canonical Array.Empty<i64> lowers through descriptor-backed managed allocation");

    let empty = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Array_Empty#generic_"))
        .expect("specialized Array.Empty function");
    let clif = empty.function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(clif.contains("beskid_rt_v5_array_construction_finish"), "{clif}");
    assert!(artifact.extern_imports.iter().any(|import| import.symbol == "array_len"));
    assert_eq!(artifact.array_static_plans.len(), 1, "one descriptor request for Array.Empty<i64>");
    assert_eq!(artifact.array_static_plans[0].element_type, beskid_queries::SemanticTypeId::I64);
    assert_eq!(artifact.array_static_plans[0].length, 0);
}

#[test]
fn ordinary_source_cannot_acquire_canonical_typed_array_allocation_authority() {
    let (input, _isa, root) = item_fixture_with_root("T[] Empty<T>() { return __array_new<T>(0); }");
    let empty = find_function_definition(input.database(), root).expect("ordinary generic function");
    let allocation_call = find_call_expression(input.database(), empty).expect("ordinary typed allocation spelling");

    assert_eq!(
        typed_array_allocation(input.database(), allocation_call).expect("typed allocation query"),
        None,
        "matching syntax outside compiler-owned Foundation source must fail closed"
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
    assert_string_content_comparison(&clif, StringComparison::Equal, "Assert.Equal<string>");
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
