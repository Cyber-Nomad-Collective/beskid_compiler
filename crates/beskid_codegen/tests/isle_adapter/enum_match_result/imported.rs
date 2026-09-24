//! Imported enum constructors, results, and scrutinee bindings across units.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, call_lowering, emit_isle_item,
    enum_layout, find_call_expression, find_function_definition, find_nodes_of_kind, isa, lower_syntax_program,
    parse_program_with_source_name, settings,
};
use super::assert_imported_result_lowering;

#[test]
fn imported_single_payload_enum_constructor_exposes_its_layout_to_isle() {
    let mut db = BeskidDatabase::default();
    let root = tempfile::tempdir().expect("project").keep();
    let main_path = root.join("Main.bd");
    let descriptor_path = root.join("Core/Syscall/Descriptor.bd");
    let stream_path = root.join("Core/Syscall/StandardStream.bd");
    let main_source = "use Core.Syscall.Descriptor;\nuse Core.Syscall.StandardStream;\nunit Main() { StandardStream stream = StandardStream::Stdout(); Descriptor descriptor = Descriptor::Standard(stream); return; }";
    let descriptor_source = "pub enum Descriptor { Standard(Core.Syscall.StandardStream stream), Raw(i64 fd), }";
    let stream_source = "pub enum StandardStream { Stdin, Stdout, Stderr, }";
    let units = [
        (main_path.clone(), main_source),
        (descriptor_path.clone(), descriptor_source),
        (stream_path.clone(), stream_source),
    ]
    .into_iter()
    .map(|(path, source)| SourceUnit {
        logical_name: path.display().to_string(),
        program: parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
            .expect("parse source"),
        origin_path: path.clone(),
        path,
        source: source.into(),
    })
    .collect::<Vec<_>>();
    let entry = SourceUnitId::new(&db, main_path.clone());
    let generation = SyntaxGenerationId(143);
    let project = ProjectSession::new(&db, root.clone(), main_path, "App".into(), "lock".into());
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
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let constructors = find_nodes_of_kind(&db, root, beskid_queries::IndexedNodeKind::EnumConstructorExpression);
    assert_eq!(constructors.len(), 2, "one StandardStream and one Descriptor constructor");
    let descriptor = constructors[1];

    assert!(
        enum_layout(&db, descriptor).expect("enum layout query").is_some(),
        "an imported single-payload enum constructor must carry its declaration layout"
    );
}

#[test]
fn imported_nullary_enum_constructor_lowers_from_an_ordinary_function_block() {
    let mut db = Box::new(BeskidDatabase::default());
    let project_root = tempfile::tempdir().expect("project").keep();
    let main_path = project_root.join("Main.bd");
    let stream_path = project_root.join("Core/Syscall/StandardStream.bd");
    let main_source =
        "use Core.Syscall.StandardStream; unit Main() { StandardStream stream = StandardStream::Stdout(); return; }";
    let stream_source = "pub enum StandardStream { Stdin, Stdout, Stderr, }";
    std::fs::create_dir_all(stream_path.parent().expect("stream parent")).expect("create stream source directory");
    std::fs::write(&main_path, main_source).expect("write main source");
    std::fs::write(&stream_path, stream_source).expect("write stream source");
    let units = [(main_path.clone(), main_source), (stream_path, stream_source)]
        .into_iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
                .expect("parse source"),
            origin_path: path.clone(),
            path,
            source: source.into(),
        })
        .collect::<Vec<_>>();
    let entry = SourceUnitId::new(&*db, main_path.clone());
    let generation = SyntaxGenerationId(145);
    let project = ProjectSession::new(&*db, project_root.clone(), main_path, "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: project_root },
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
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe imported enum input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let main = find_function_definition(input.database(), root).expect("Main item");

    emit_isle_item(&input, isa.as_ref(), main)
        .expect("ordinary function blocks lower imported nullary enum constructors");
}

#[test]
fn imported_result_write_with_lowers_through_an_ordinary_function_block_match() {
    let mut db = Box::new(BeskidDatabase::default());
    let project_root = tempfile::tempdir().expect("project").keep();
    let main_path = project_root.join("Main.bd");
    let descriptor_path = project_root.join("Core/Syscall/Descriptor.bd");
    let stream_path = project_root.join("Core/Syscall/StandardStream.bd");
    let main_source = "use Core.Syscall.Descriptor; use Core.Syscall.StandardStream; use Core.Syscall.WriteRequest; use Core.Syscall.WriteWith; use Core.Syscall.Result; unit Main(string text) { StandardStream stream = StandardStream::Stdout(); Descriptor descriptor = Descriptor::Standard(stream); Result result = WriteWith(WriteRequest { descriptor: descriptor, data: text }); match result { Result::Ok(_) => {}, Result::Error(_) => {}, }; return; }";
    let descriptor_source = "pub enum Descriptor { Standard(Core.Syscall.StandardStream stream), Raw(i64 fd), } pub type WriteRequest { Descriptor descriptor, string data } pub enum Result { Ok(i64 value), Error(i64 error), } pub Result WriteWith(WriteRequest request) { return Result::Ok(0_i64); }";
    let stream_source = "pub enum StandardStream { Stdin, Stdout, Stderr, }";
    std::fs::create_dir_all(descriptor_path.parent().expect("descriptor parent"))
        .expect("create descriptor source directory");
    std::fs::write(&main_path, main_source).expect("write main source");
    std::fs::write(&descriptor_path, descriptor_source).expect("write descriptor source");
    std::fs::write(&stream_path, stream_source).expect("write stream source");
    let units = [(main_path.clone(), main_source), (descriptor_path, descriptor_source), (stream_path, stream_source)]
        .into_iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
                .expect("parse source"),
            origin_path: path.clone(),
            path,
            source: source.into(),
        })
        .collect::<Vec<_>>();
    let entry = SourceUnitId::new(&*db, main_path.clone());
    let generation = SyntaxGenerationId(146);
    let project = ProjectSession::new(&*db, project_root.clone(), main_path, "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: project_root },
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
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe imported enum input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let main = find_function_definition(input.database(), root).expect("Main item");
    let call = find_call_expression(input.database(), main).expect("WriteWith call");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(input.database(), call).expect("WriteWith call lowering").expect("direct WriteWith call")
    else {
        panic!("WriteWith must be a direct imported call");
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: declaration, symbol: "WriteWith".into() },
        ],
    )
    .expect("module artifact services lower imported Result WriteWith and string data");
    let main_function =
        artifact.functions.iter().find(|function| function.name == "Main").expect("Main function in artifact");
    let clif = main_function.function.display().to_string();
    assert!(clif.contains("call"), "{clif}");
    assert_eq!(clif.matches("brif").count(), 2, "Result arms must lower as ordered tag tests: {clif}");
}

#[test]
fn imported_call_scrutinee_binding_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.method, \"POST\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { string name, } pub type Request { pub string method, pub Header[] headers, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_local_scrutinee_binding_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { Result<Request, HttpError> parsed = Parse(1_i64); match parsed { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.method, \"POST\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { string name, } pub type Request { pub string method, pub Header[] headers, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_call_scrutinee_binding_indexed_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.headers[0].name, \"host\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_call_scrutinee_binding_indexed_bytes_infer_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.body[4], 101_u8); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_local_scrutinee_binding_indexed_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { Result<Request, HttpError> parsed = Parse(1_i64); match parsed { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.headers[0].name, \"host\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_result_binding_array_field_owns_a_canonical_append() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Core.Collections.Array; use Http.Errors; use Http.Requests; bool Main(Result<Request, HttpError> head, Header extra) { return match head { Result::Error(_) => false, Result::Ok(request) => { Array.Append<Header>(request.headers, extra); return true; }, }; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Core/Collections/Array.bd", "pub T[] Append<T>(mut T[] values, T value) { return values; }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Header { string name, } pub type Request { string method, Header[] headers, }"),
    ]);
}
