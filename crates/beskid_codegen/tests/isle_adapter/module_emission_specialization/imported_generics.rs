//! Generic nominal methods, qualified imported generic calls, and imported unit calls.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, find_function_definition,
    find_function_definitions, find_node, find_test_definition, isa, item_fixture_with_root, lower_syntax_program,
    parse_program_with_source_name, settings,
};
use super::{StringComparison, assert_string_content_comparison};

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
                origin_path: main_path.clone(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: "Testing.Assert".into(),
                origin_path: assert_path.clone(),
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
