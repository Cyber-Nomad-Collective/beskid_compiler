//! Nested call operands, parameter boundaries, service spelling, and whole-program emission.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, JITBuilder, JITModule, Linkage, Module, ModuleIndex, NodeFacts, ProgramAssembly,
    ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem,
    TargetMetadata, build_typed_program, default_libcall_names, emit_syntax_program, find_call_expression,
    find_function_definition, find_function_definitions, isa, item_fixture_with_root, lower_syntax_program,
    parse_program_with_source_name, settings,
};

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
            origin_path: (*path).clone(),
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
fn nested_direct_call_adapts_scalars_at_the_exact_inner_parameter_boundary() {
    let mut db = Box::new(BeskidDatabase::default());
    let project_root = tempfile::tempdir().expect("project").keep();
    let root = project_root.join("src");
    let main_path = root.join("Controls/RenderContext.bd");
    let cursor_path = root.join("Ansi/Cursor.bd");
    let sources = [
        (
            &main_path,
            "use Ansi.Cursor;\npub string MoveTo(i32 row, i32 col) { return Ansi.Cursor.IntoSequence(Ansi.Cursor.Position(Ansi.Cursor.Start(), row, col)); }",
        ),
        (
            &cursor_path,
            "pub type CursorBuilder { string parts } pub CursorBuilder Start() { return CursorBuilder { parts: \"\" }; } pub CursorBuilder Position(CursorBuilder self, i64 row, i64 col) { return CursorBuilder { parts: self.parts }; } pub string IntoSequence(CursorBuilder self) { return self.parts; }",
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
            origin_path: (*path).clone(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 path"), source).expect("parse"),
        })
        .collect::<Vec<_>>();
    let generation = SyntaxGenerationId(141);
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
    let project = ProjectSession::new(&*db, project_root, main_path, "nested-direct".into(), "nested-direct".into());
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&units[0].program, generation);
    let cursor_index = SyntaxIndex::from_program(&units[1].program, generation);
    let main = AstNodeKey {
        unit: main_unit,
        generation,
        node: main_index.ids_of_kind(beskid_queries::IndexedNodeKind::FunctionDefinition).next().expect("MoveTo"),
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
            SyntaxModuleItem { key: cursor_functions[2], symbol: "IntoSequence".into() },
            SyntaxModuleItem { key: main, symbol: "MoveTo".into() },
        ],
    )
    .expect("the inner direct call must adapt its scalar arguments before its result reaches the enclosing call");
    let move_to = artifact.functions.iter().find(|function| function.name == "MoveTo").expect("MoveTo artifact");
    let clif = move_to.function.display().to_string();
    assert_eq!(
        clif.matches("sextend.i64").count(),
        2,
        "both i32 coordinates must widen at the i64 call boundary:\n{clif}"
    );
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
