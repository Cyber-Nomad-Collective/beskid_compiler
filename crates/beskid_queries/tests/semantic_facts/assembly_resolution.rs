use super::support::{key, key_at_start};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, CompletionContext, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_typed_program, call_lowering, completion_candidates, direct_callees, enum_constructor, reachable_items,
    resolved_item,
};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn qualified_import_resolution_uses_registered_dependency_syntax() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/qualified-import/project/src");
    let main_path = root.join("Main.bd");
    let tools_path = root.join("Lib/Tools.bd");
    let main_source = "use Lib.Tools as Utility;\ni32 Main() { Utility.Member(); return Utility.Helper(); }";
    let tools_source = "pub i32 Helper() { return 1; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let tools_program =
        expand_program(parse_program(tools_source).expect("tools parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(17);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: tools_path.display().to_string(),
                path: tools_path.clone(),
                source: tools_source.to_string(),
                program: tools_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let tools_unit = SourceUnitId::new(&db, tools_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let tools_index = SyntaxIndex::from_program(&tools_program, generation);
    let reference = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Utility.Helper").expect("qualified call"),
    );
    let declaration = key(tools_unit, generation, &tools_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        resolved_item(&db, reference).expect("qualified resolution"),
        Some(beskid_queries::ResolvedItem { declaration })
    );
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    assert_eq!(call_lowering(&db, call).expect("qualified direct call"), Some(beskid_queries::CallLowering::Dynamic));
    let direct_call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);
    assert_eq!(
        call_lowering(&db, direct_call).expect("qualified direct call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);
    let program = key(main_unit, generation, &main_index, NodeKind::Program, 0);
    assert_eq!(
        reachable_items(&db, program, main).expect("cross-unit reachability").expect("cross-unit graph").as_ref(),
        &[main, declaration]
    );
    let member_cursor = main_source.find("Utility.Helper").expect("qualified call") + "Utility.".len();
    let completion_key = key(main_unit, generation, &main_index, NodeKind::Program, 0);
    let members = completion_candidates(
        &db,
        completion_key,
        CompletionContext {
            cursor: member_cursor,
            replacement_start: member_cursor,
            replacement_end: member_cursor + "Helper".len(),
        },
    )
    .expect("member completion")
    .expect("member candidates");
    assert_eq!(members.iter().map(|candidate| candidate.label.as_ref()).collect::<Vec<_>>(), vec!["Helper"]);
    assert_eq!(
        resolved_item(&db, AstNodeKey { generation: SyntaxGenerationId(16), ..reference }).expect("stale generation"),
        None
    );
}

#[test]
fn qualified_import_resolution_follows_public_module_reexports() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/public-reexport/project/src");
    let main_path = root.join("Main.bd");
    let parser_path = root.join("Core/Text/Parser.bd");
    let result_path = root.join("Core/Text/Parser/Result.bd");
    let private_path = root.join("Core/Text/Parser/Private.bd");
    let main_source =
        "use Core.Text.Parser;\ni32 Main() { Parser.IsOk(); Parser.Hidden(); Parser.TextParseResult::Ok(); return 1; }";
    let parser_source = "pub use Core.Text.Parser.Result;\nuse Core.Text.Parser.Private;";
    let result_source = "pub i32 IsOk() { return 1; }\npub enum TextParseResult { Ok() }";
    let private_source = "pub i32 Hidden() { return 1; }";
    let sources = [
        (&main_path, main_source),
        (&parser_path, parser_source),
        (&result_path, result_source),
        (&private_path, private_source),
    ];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let generation = SyntaxGenerationId(18);
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
    let main_unit = SourceUnitId::new(&db, main_path);
    let result_unit = SourceUnitId::new(&db, result_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&units[0].program, generation);
    let result_index = SyntaxIndex::from_program(&units[2].program, generation);
    let is_ok = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.IsOk").expect("public function"),
    );
    let is_ok_declaration = key(result_unit, generation, &result_index, NodeKind::FunctionDefinition, 0);
    assert_eq!(
        resolved_item(&db, is_ok).expect("public re-export"),
        Some(beskid_queries::ResolvedItem { declaration: is_ok_declaration })
    );
    let hidden = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.Hidden").expect("private function"),
    );
    assert_eq!(resolved_item(&db, hidden).expect("private import"), None);
    let constructor = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::EnumConstructorExpression,
        main_source.find("Parser.TextParseResult").expect("re-exported type"),
    );
    assert!(enum_constructor(&db, constructor).expect("re-exported type").is_some());
}

#[test]
fn imported_assembly_module_call_resolves_through_its_use_binding() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/fully-qualified-module/project/src");
    let terminal_path = root.join("Platform/Terminal.bd");
    let string_path = root.join("Core/String/String.bd");
    let terminal_source = "use Core.String;\nbool EnvFlagSet(string value) { return String.IsEmpty(value); }";
    let string_source = "pub bool IsEmpty(string value) { return value == \"\"; }";
    let terminal_program =
        expand_program(parse_program(terminal_source).expect("terminal parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let string_program =
        expand_program(parse_program(string_source).expect("string parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(25);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: terminal_path.display().to_string(),
                path: terminal_path.clone(),
                source: terminal_source.to_string(),
                program: terminal_program.clone(),
            },
            SourceUnit {
                logical_name: string_path.display().to_string(),
                path: string_path.clone(),
                source: string_source.to_string(),
                program: string_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let terminal_unit = SourceUnitId::new(&db, terminal_path);
    let string_unit = SourceUnitId::new(&db, string_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        terminal_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let terminal_index = SyntaxIndex::from_program(&terminal_program, generation);
    let string_index = SyntaxIndex::from_program(&string_program, generation);
    let call = key(terminal_unit, generation, &terminal_index, NodeKind::CallExpression, 0);
    let declaration = key(string_unit, generation, &string_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("imported module call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn hub_declaration_shadows_the_same_name_reached_through_its_public_reexport() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/hub-shadowing/project/src");
    let casing_path = root.join("Core/Text/Casing.bd");
    let hub_path = root.join("Core/String/String.bd");
    let core_path = root.join("Core/String/Core.bd");
    // The Corelib `Core.String` hub declares flat helpers *and* re-exports the child module
    // those helpers delegate to, so both units export `Len`. The hub's own declaration is the
    // nearer route and must win instead of leaving `String.Len` permanently ambiguous.
    let casing_source = "use Core.String;\ni64 Width(string text) { return String.Len(text); }";
    let hub_source =
        "pub mod Core.String.Core;\nuse Core.String.Core;\npub i64 Len(string text) { return Core.Len(text); }";
    let core_source = "pub i64 Len(string text) { return 1; }";
    let sources = [(&casing_path, casing_source), (&hub_path, hub_source), (&core_path, core_source)];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let generation = SyntaxGenerationId(31);
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
    let casing_unit = SourceUnitId::new(&db, casing_path);
    let hub_unit = SourceUnitId::new(&db, hub_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        casing_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let casing_index = SyntaxIndex::from_program(&units[0].program, generation);
    let hub_index = SyntaxIndex::from_program(&units[1].program, generation);
    let call = key(casing_unit, generation, &casing_index, NodeKind::CallExpression, 0);
    let hub_declaration = key(hub_unit, generation, &hub_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("hub helper call"),
        Some(beskid_queries::CallLowering::Direct(hub_declaration))
    );
}

#[test]
fn imported_type_qualified_static_call_resolves_to_its_exact_syntax_item() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-type-qualified/project/src");
    let main_path = root.join("Main.bd");
    let progress_path = root.join("Console/Controls/ProgressBar.bd");
    let main_source = "use Console.Controls.ProgressBar;\ni32 Main() { return ProgressBar.ProgressBar.New(); }";
    let progress_source = "pub type ProgressBar { i32 percent }\npub i32 New() { return 1; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let progress_program =
        expand_program(parse_program(progress_source).expect("progress parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(54);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: progress_path.display().to_string(),
                path: progress_path.clone(),
                source: progress_source.to_string(),
                program: progress_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let progress_unit = SourceUnitId::new(&db, progress_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let progress_index = SyntaxIndex::from_program(&progress_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(progress_unit, generation, &progress_index, NodeKind::FunctionDefinition, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("imported type-qualified static call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(direct_callees(&db, main).expect("imported type-qualified call graph"), Some(Arc::from([declaration])));
}

#[test]
fn syntax_facts_resolve_core_output_writeline_without_hir() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/core-output-writeline/project/src");
    let main_path = root.join("Main.bd");
    let output_path = root.join("Core/Output/Output.bd");
    let main_source = "use Core.Output;\nunit Main() { Core.Output.WriteLine(\"hello\"); return; }";
    let output_source = "pub unit WriteLine(string text) { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let output_program =
        expand_program(parse_program(output_source).expect("Core.Output parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(55);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                path: output_path.clone(),
                source: output_source.to_string(),
                program: output_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let output_unit = SourceUnitId::new(&db, output_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let output_index = SyntaxIndex::from_program(&output_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(output_unit, generation, &output_index, NodeKind::FunctionDefinition, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("Core.Output.WriteLine syntax lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        direct_callees(&db, main).expect("Core.Output.WriteLine syntax call graph"),
        Some(Arc::from([declaration]))
    );
}

#[test]
fn syntax_facts_resolve_core_output_writeline_via_import_alias() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/core-output-writeline-import-alias/project/src");
    let main_path = root.join("Main.bd");
    let output_path = root.join("Core/Output/Output.bd");
    let main_source = "use Core.Output as Output;\nunit Main() { Output.WriteLine(\"hello\"); return; }";
    let output_source = "pub unit WriteLine(string text) { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let output_program =
        expand_program(parse_program(output_source).expect("Core.Output parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(57);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                path: output_path.clone(),
                source: output_source.to_string(),
                program: output_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let output_unit = SourceUnitId::new(&db, output_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let output_index = SyntaxIndex::from_program(&output_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(output_unit, generation, &output_index, NodeKind::FunctionDefinition, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("Output.WriteLine syntax lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(direct_callees(&db, main).expect("Output.WriteLine syntax call graph"), Some(Arc::from([declaration])));
}

#[test]
fn syntax_facts_do_not_resolve_core_output_writeline_through_alias() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/core-output-writeline-alias/project/src");
    let main_path = root.join("Main.bd");
    let output_path = root.join("Core/Output/Output.bd");
    let main_source = "use Core.Output as Output;\nunit Main() { Core.Output.WriteLine(\"hello\"); return; }";
    let output_source = "pub unit WriteLine(string text) { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let output_program =
        expand_program(parse_program(output_source).expect("Core.Output parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(56);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                path: output_path,
                source: output_source.to_string(),
                program: output_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("aliased Core.Output.WriteLine syntax lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
    assert_eq!(
        direct_callees(&db, main).expect("aliased Core.Output.WriteLine syntax call graph"),
        Some(Arc::from([]))
    );
}

#[test]
fn qualified_import_alias_ambiguity_has_no_syntax_item_fact() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/qualified-import-ambiguity/project/src");
    let main_path = root.join("Main.bd");
    let left_path = root.join("Lib/Tools.bd");
    let right_path = root.join("Other/Tools.bd");
    let main_source = "use Lib.Tools as Utility;\nuse Other.Tools as Utility;\ni32 Main() { return Utility.Helper(); }";
    let tools_source = "pub i32 Helper() { return 1; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let tools_program =
        expand_program(parse_program(tools_source).expect("tools parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(38);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: left_path.display().to_string(),
                path: left_path,
                source: tools_source.to_string(),
                program: tools_program.clone(),
            },
            SourceUnit {
                logical_name: right_path.display().to_string(),
                path: right_path,
                source: tools_source.to_string(),
                program: tools_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let reference = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Utility.Helper").expect("qualified call"),
    );
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);

    assert_eq!(resolved_item(&db, reference).expect("ambiguity"), None);
    assert!(call_lowering(&db, call).is_err());
}

#[test]
fn unqualified_import_resolution_requires_one_registered_syntax_target() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/unqualified-import/project/src");
    let main_path = root.join("Main.bd");
    let tools_path = root.join("Lib/Tools.bd");
    let main_source = "use Lib.Tools;\ni32 Main() { return Helper(); }";
    let tools_source = "pub i32 Helper() { return 1; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let tools_program =
        expand_program(parse_program(tools_source).expect("tools parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(18);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: tools_path.display().to_string(),
                path: tools_path.clone(),
                source: tools_source.to_string(),
                program: tools_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let tools_unit = SourceUnitId::new(&db, tools_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let tools_index = SyntaxIndex::from_program(&tools_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let helper = key(tools_unit, generation, &tools_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("unqualified imported call"),
        Some(beskid_queries::CallLowering::Direct(helper))
    );
}
