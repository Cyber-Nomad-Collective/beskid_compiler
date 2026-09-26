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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: tools_path.display().to_string(),
                origin_path: tools_path.clone(),
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
            origin_path: (*path).clone(),
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
                origin_path: terminal_path.clone(),
                path: terminal_path.clone(),
                source: terminal_source.to_string(),
                program: terminal_program.clone(),
            },
            SourceUnit {
                logical_name: string_path.display().to_string(),
                origin_path: string_path.clone(),
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
            origin_path: (*path).clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: progress_path.display().to_string(),
                origin_path: progress_path.clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                origin_path: output_path.clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                origin_path: output_path.clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: output_path.display().to_string(),
                origin_path: output_path.clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: left_path.display().to_string(),
                origin_path: left_path.clone(),
                path: left_path,
                source: tools_source.to_string(),
                program: tools_program.clone(),
            },
            SourceUnit {
                logical_name: right_path.display().to_string(),
                origin_path: right_path.clone(),
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
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: tools_path.display().to_string(),
                origin_path: tools_path.clone(),
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

/// E1105 is judged against the same assembled module registry import registration reads, and
/// only for units that own a judged item, once per unit. A `use` of a top-level item of an
/// assembled module resolves. `check_items` rejects it.
#[test]
fn unresolved_imports_are_judged_only_for_units_of_judged_items() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/unresolved-import/project/src");
    let main_path = root.join("Main.bd");
    let tools_path = root.join("Lib/Tools.bd");
    let main_source = "use Lib.Tools as Utility;\nuse Lib.Missing;\nuse Lib.Tools.Helper;\nuse Lib.Tools.Absent;\ni32 Main() { return Utility.Helper(); }";
    let tools_source = "use Absent.Thing;\npub i32 Helper() { return 1; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let tools_program =
        expand_program(parse_program(tools_source).expect("tools parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(19);
    let unit =
        |path: &PathBuf, source: &str, program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>| {
            SourceUnit {
                logical_name: path.display().to_string(),
                origin_path: path.clone(),
                path: path.clone(),
                source: source.to_string(),
                program: program.clone(),
            }
        };
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![unit(&main_path, main_source, &main_program), unit(&tools_path, tools_source, &tools_program)]),
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
    let main_root = key(main_unit, generation, &main_index, NodeKind::Program, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);
    let helper = key(tools_unit, generation, &tools_index, NodeKind::FunctionDefinition, 0);

    let imports = beskid_queries::unresolved_imports(&db, main_root).expect("query").expect("unit root");
    // `Lib.Tools.Helper` names a top-level item of the assembled `Lib.Tools` module; `Lib.Tools.Absent`
    // names nothing that module declares.
    assert_eq!(
        imports.iter().map(|import| import.path.as_ref()).collect::<Vec<_>>(),
        vec!["Lib.Missing", "Lib.Tools.Absent"]
    );
    assert_eq!(imports[0].site, key(main_unit, generation, &main_index, NodeKind::UseDeclaration, 1));
    assert_eq!(imports[1].site, key(main_unit, generation, &main_index, NodeKind::UseDeclaration, 3));

    let codes = |items: &[AstNodeKey]| {
        beskid_queries::check_items(&db, items)
            .expect_err("an unresolved import rejects the request")
            .iter()
            .map(|finding| (finding.kind.code(), finding.kind.message()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        codes(&[main]),
        vec![
            ("E1105", "unknown import path `Lib.Missing`".to_string()),
            ("E1105", "unknown import path `Lib.Tools.Absent`".to_string()),
        ]
    );
    assert_eq!(codes(&[helper]), vec![("E1105", "unknown import path `Absent.Thing`".to_string())]);
    assert_eq!(codes(&[main, main]).len(), 2, "a unit is judged once");
    assert_eq!(codes(&[main, helper]).len(), 3, "each owning unit is judged");
}

#[test]
fn std_app_rejects_bare_core_import_while_corelib_shard_accepts_it() {
    let mut db = BeskidDatabase::default();
    let host_root = PathBuf::from("/tmp/std-import-scope/app/src");
    let shard_root = PathBuf::from("/tmp/std-import-scope/corelib/Src");
    let main_path = host_root.join("Main.bd");
    let qualified_path = host_root.join("Qualified.bd");
    let results_path = shard_root.join("Core/Results.bd");
    let main_source = "use Std.Core.Results;\nuse Core.Results;\nuse Core.Results.Result;\ni32 Main() { return 0; }";
    let qualified_source = r#"
i32 BareType(Core.Results.Widget value) { return 0; }
i32 BareContract(Core.Results.Reader reader) { return 0; }
i32 StdType(Std.Core.Results.Widget value) { return 0; }
i32 StdContract(Std.Core.Results.Reader reader) { return 0; }
"#;
    let results_source = r#"
use Core.Results;
use Core.Results.Result;
pub enum Result { Ok() }
pub type Widget {}
pub contract Reader { i32 Read(); }
i32 ShardType(Core.Results.Widget value) { return 0; }
i32 ShardContract(Core.Results.Reader reader) { return 0; }
"#;
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let qualified_program =
        expand_program(parse_program(qualified_source).expect("qualified parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let results_program =
        expand_program(parse_program(results_source).expect("results parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(20);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: host_root.clone() },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_results".into()),
                source_root: shard_root.clone(),
            }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: results_path.display().to_string(),
                origin_path: results_path.clone(),
                path: results_path.clone(),
                source: results_source.to_string(),
                program: results_program.clone(),
            },
            SourceUnit {
                logical_name: qualified_path.display().to_string(),
                origin_path: qualified_path.clone(),
                path: qualified_path.clone(),
                source: qualified_source.to_string(),
                program: qualified_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        true,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let results_unit = SourceUnitId::new(&db, results_path);
    let qualified_unit = SourceUnitId::new(&db, qualified_path);
    let project = ProjectSession::new(
        &db,
        host_root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let results_index = SyntaxIndex::from_program(&results_program, generation);
    let qualified_index = SyntaxIndex::from_program(&qualified_program, generation);
    let main_root = key(main_unit, generation, &main_index, NodeKind::Program, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 0);
    let results_root = key(results_unit, generation, &results_index, NodeKind::Program, 0);

    let main_imports = beskid_queries::unresolved_imports(&db, main_root).expect("main query").expect("main root");
    assert_eq!(
        main_imports.iter().map(|import| import.path.as_ref()).collect::<Vec<_>>(),
        vec!["Core.Results", "Core.Results.Result"]
    );
    let findings = beskid_queries::check_items(&db, &[main]).expect_err("App bare Core imports must fail E1105");
    assert_eq!(findings.iter().map(|finding| finding.kind.code()).collect::<Vec<_>>(), vec!["E1105", "E1105"]);
    let shard_imports =
        beskid_queries::unresolved_imports(&db, results_root).expect("shard query").expect("shard root");
    assert!(shard_imports.is_empty(), "corelib shard must resolve its own bare Core import");

    let unresolved_type = |unit, index: &SyntaxIndex, ordinal| {
        let function = key(unit, generation, index, NodeKind::FunctionDefinition, ordinal);
        beskid_queries::unresolved_type_reference(&db, function).expect("nominal type query")
    };
    let bare_type = unresolved_type(qualified_unit, &qualified_index, 0).map(|reference| reference.name.to_string());
    let bare_contract =
        unresolved_type(qualified_unit, &qualified_index, 1).map(|reference| reference.name.to_string());
    assert_eq!((bare_type, bare_contract), (Some("Widget".into()), Some("Reader".into())));
    assert!(unresolved_type(qualified_unit, &qualified_index, 2).is_none(), "Std-qualified type must resolve");
    assert!(unresolved_type(qualified_unit, &qualified_index, 3).is_none(), "Std-qualified contract must resolve");
    assert!(unresolved_type(results_unit, &results_index, 0).is_none(), "shard-local type must resolve");
    assert!(unresolved_type(results_unit, &results_index, 1).is_none(), "shard-local contract must resolve");
}

#[test]
fn std_app_spawn_fiber_handle_and_parameter_ownership_use_canonical_module_path() {
    let mut db = BeskidDatabase::default();
    let host_root = PathBuf::from("/tmp/std-fiber-scope/app/src");
    let shard_root = PathBuf::from("/tmp/std-fiber-scope/corelib/Src");
    let main_path = host_root.join("Main.bd");
    let fiber_path = shard_root.join("Concurrency/Fiber.bd");
    let main_source = r#"
i64 Compute() { return 42_i64; }
unit Main(Std.Concurrency.Fiber<i64> parameter) {
    let child = spawn Compute();
    parameter.Join();
    parameter.Join();
    return;
}
"#;
    let fiber_source = r#"
pub type Fiber<T> { i64 handle, pub unit Join() { return; } }
unit ShardUse(Concurrency.Fiber<i64> parameter) {
    parameter.Join();
    parameter.Join();
    return;
}
"#;
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let fiber_program =
        expand_program(parse_program(fiber_source).expect("fiber parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(21);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: host_root.clone() },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_concurrency".into()),
                source_root: shard_root,
            }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.to_string(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: fiber_path.display().to_string(),
                origin_path: fiber_path.clone(),
                path: fiber_path.clone(),
                source: fiber_source.to_string(),
                program: fiber_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        true,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let fiber_unit = SourceUnitId::new(&db, fiber_path);
    let project = ProjectSession::new(
        &db,
        host_root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let fiber_index = SyntaxIndex::from_program(&fiber_program, generation);
    let spawn = key(main_unit, generation, &main_index, NodeKind::SpawnExpression, 0);
    let main = key(main_unit, generation, &main_index, NodeKind::FunctionDefinition, 1);
    let fiber = key(fiber_unit, generation, &fiber_index, NodeKind::TypeDefinition, 0);
    let shard_use = key(fiber_unit, generation, &fiber_index, NodeKind::FunctionDefinition, 0);

    let handle = beskid_queries::spawn_handle_type(&db, spawn);
    let ownership = beskid_queries::callable_fiber_ownership(&db, main);
    assert!(
        handle.as_ref().ok().and_then(Option::as_ref).is_some()
            && ownership.as_ref().ok().and_then(Option::as_ref).is_some_and(|fact| {
                fact.diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind == beskid_queries::SpawnDiagnosticKind::UseAfterMove)
            }),
        "handle={handle:?} ownership={ownership:?}"
    );
    let handle = handle.expect("spawn handle query").expect("spawn handle");
    assert_eq!(handle.declaration, fiber);
    assert_eq!(handle.payload.argument, beskid_queries::SemanticTypeId::I64);
    let shard_ownership = beskid_queries::callable_fiber_ownership(&db, shard_use)
        .expect("shard ownership query")
        .expect("shard ownership");
    assert!(
        shard_ownership
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == beskid_queries::SpawnDiagnosticKind::UseAfterMove),
        "shard ownership={shard_ownership:?}"
    );
}
