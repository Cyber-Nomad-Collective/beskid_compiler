use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SyntaxGenerationId,
    SourceUnitId, SyntaxIndex, SyntaxModuleItem, TargetMetadata, build_typed_program, find_function_definition,
    find_function_definitions, find_node, isa, item_fixture_with_root, lower_syntax_program,
    parse_program_with_source_name, settings,
};

fn imported_list_fixture(
    list_source: &str,
    dependencies: &[(&str, &str)],
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let root = tempfile::tempdir().expect("project").keep();
    let list_path = root.join("Core/Collections/List.bd");
    let mut sources = vec![(list_path.clone(), list_source.to_owned())];
    sources.extend(dependencies.iter().map(|(path, source)| (root.join(path), (*source).to_owned())));
    for (path, source) in &sources {
        std::fs::create_dir_all(path.parent().expect("source parent")).expect("create source parent");
        std::fs::write(path, source).expect("write source");
    }
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: path.clone(),
            source: source.clone(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 path"), source).expect("parse source"),
        })
        .collect::<Vec<_>>();
    let list_program = units[0].program.clone();
    let generation = SyntaxGenerationId(177);
    let project = ProjectSession::new(&*db, root.clone(), list_path, "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: Vec::new(),
        },
        Arc::from(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let list_unit = typed.entry;
    let root = AstNodeKey { unit: list_unit, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe imported List input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let _index = SyntaxIndex::from_program(&list_program, generation);
    (input, isa, root)
}

fn imported_function(input: &CodegenInput<'_>, logical_name: &str) -> AstNodeKey {
    let unit = input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| unit.logical_name.ends_with(logical_name))
        .expect("imported source unit");
    let root = AstNodeKey {
        unit: SourceUnitId::new(input.database(), unit.path.clone()),
        generation: input.typed_program().generation,
        node: AstNodeId(0),
    };
    find_function_definitions(input.database(), root)[0]
}

#[test]
fn specialized_generic_method_lowers_an_implicit_array_field_local_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type List<T> {
    T[] storage,
    unit CopyStorage() {
        mut T[] nextStorage = storage;
        return;
    }
}
unit Main(List<i64> list) {
    list.CopyStorage();
    return;
}
"#,
    );
    let method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.CopyStorage method");
    let main = find_function_definition(input.database(), root).expect("Main item");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "List_CopyStorage".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("implicit array field local lowers through the generic method specialization");

    let method = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("List_CopyStorage#generic_"))
        .expect("specialized CopyStorage body");
    assert!(method.function.display().to_string().contains("load.i64"));
}

#[test]
fn specialized_generic_method_lowers_bounds_over_an_implicit_scalar_field_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type List<T> {
    i64 count,
    bool Outside(i64 index) {
        return index < 0 || index >= count;
    }
}
unit Main(List<i64> list) {
    list.Outside(0_i64);
    return;
}
"#,
    );
    let method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.Outside method");
    let main = find_function_definition(input.database(), root).expect("Main item");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "List_Outside".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("implicit scalar field bounds lower through the generic method specialization");

    let method = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("List_Outside#generic_"))
        .expect("specialized Outside body");
    let clif = method.function.display().to_string();
    assert!(clif.contains("load.i64"), "{clif}");
    assert!(clif.contains("icmp"), "{clif}");
}

#[test]
fn specialized_generic_method_lowers_nested_array_append_with_its_enclosing_element_type() {
    let list_source = r#"
use Core.Collections.Array;
pub type List<T> {
    T[] storage,
    pub List<T> Push(T value) {
        mut T[] nextStorage = storage;
        Array.Append<T>(nextStorage, value);
        return List<T> { storage: nextStorage };
    }
}
unit Main(List<i64> list) { list.Push(7_i64); return; }
"#;
    let array_source = "pub T[] Append<T>(mut T[] values, T value) { return values; }";
    let (input, isa, root) = imported_list_fixture(list_source, &[("Core/Collections/Array.bd", array_source)]);
    let push =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("List.Push method");
    let main = find_function_definition(input.database(), root).expect("Main function");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: push, symbol: "List_Push".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("nested Array.Append<T> lowers with the List<i64> specialization");
}

#[test]
fn specialized_generic_method_lowers_contextual_result_error_constructor() {
    let list_source = r#"
use Core.Results;
pub type List<T> {
    pub Result<T, string> Get(i64 index) {
        return Result::Error("missing");
    }
}
unit Main(List<i64> list) { list.Get(0_i64); return; }
"#;
    let result_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let (input, isa, root) = imported_list_fixture(list_source, &[("Core/Results/Results.bd", result_source)]);
    let get =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("List.Get method");
    let main = find_function_definition(input.database(), root).expect("Main function");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: get, symbol: "List_Get".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("Result::Error inherits the specialized List<i64>.Get return layout");
}

#[test]
fn specialized_generic_method_returns_its_implicit_receiver() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type List<T> {
    List<T> Pop() { return self; }
}
unit Main(List<i64> list) { list.Pop(); return; }
"#,
    );
    let pop = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.Pop method");
    let main = find_function_definition(input.database(), root).expect("Main function");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: pop, symbol: "List_Pop".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("self lowers through the implicit method receiver slot");
}

#[test]
fn specialized_generic_method_lowers_nested_generic_call_as_enum_payload() {
    let list_source = r#"
use Core.Collections.Array;
use Core.Results;
pub type List<T> {
    T[] storage,
    pub Result<T, string> Get(i64 index) {
        return Result::Ok(Array.Get<T>(storage, index));
    }
}
unit Main(List<i64> list) { list.Get(0_i64); return; }
"#;
    let array_source = "pub T Get<T>(T[] values, i64 index) { return values[index]; }";
    let result_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let (input, isa, root) = imported_list_fixture(
        list_source,
        &[
            ("Core/Collections/Array.bd", array_source),
            ("Core/Results/Results.bd", result_source),
        ],
    );
    let get = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.Get method");
    let main = find_function_definition(input.database(), root).expect("Main function");
    let array_get = imported_function(&input, "Core/Collections/Array.bd");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: get, symbol: "List_Get".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: array_get, symbol: "Array_Get".into() },
        ],
    )
    .expect("Array.Get<T> lowers with the enclosing specialization as Result::Ok payload");
}

#[test]
fn specialized_generic_method_lowers_nested_generic_return_call() {
    let list_source = r#"
use Core.Collections.Array;
pub type List<T> {
    T[] storage,
    pub T Get(i64 index) { return Array.Get<T>(storage, index); }
}
unit Main(List<i64> list) { list.Get(0_i64); return; }
"#;
    let array_source = "pub T Get<T>(T[] values, i64 index) { return values[index]; }";
    let (input, isa, root) =
        imported_list_fixture(list_source, &[("Core/Collections/Array.bd", array_source)]);
    let get = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.Get method");
    let main = find_function_definition(input.database(), root).expect("Main function");
    let array_get = imported_function(&input, "Core/Collections/Array.bd");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: get, symbol: "List_Get".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: array_get, symbol: "Array_Get".into() },
        ],
    )
    .expect("Array.Get<T> lowers with the enclosing List<i64> specialization");
}
