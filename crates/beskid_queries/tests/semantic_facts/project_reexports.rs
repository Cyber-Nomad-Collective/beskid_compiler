use super::support::{assert_unavailable, key, key_at_start};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program, enum_constructor,
    resolved_item,
};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn qualified_import_resolution_follows_public_reexports_and_declared_modules() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/public-module-export/project/src");
    let main_path = root.join("Main.bd");
    let parser_path = root.join("Core/Text/Parser.bd");
    let result_path = root.join("Core/Text/Parser/Result.bd");
    let private_path = root.join("Core/Text/Parser/Private.bd");
    let regex_path = root.join("Core/Text/Regex.bd");
    let generated_path = root.join("Core/Text/Regex/Generated.bd");
    let main_source = "use Core.Text.Parser;\nuse Core.Text.Regex;\ni32 Main() { Parser.HiddenRecord record = Parser.HiddenRecord { value: 1 }; Parser.IsOk(); Parser.PrivateTerminal(); Parser.Private.Hidden(); Parser.Second.IsOk(); Regex.Generated.ParseDigit(); Core.Text.Regex.Generated.ParseDigit(); Parser.TextParseResult::Ok(); Parser.HiddenType::Nope(); return 1; }";
    let parser_source =
        "pub use Core.Text.Parser.Result;\npub use Core.Text.Parser.Result as Second;\nmod Core.Text.Parser.Private;";
    let result_source = "pub i32 IsOk() { return 1; }\ni32 PrivateTerminal() { return 1; }\npub enum TextParseResult { Ok() }\nenum HiddenType { Nope() }\ntype HiddenRecord { i32 value }";
    let private_source = "pub i32 Hidden() { return 1; }";
    let regex_source = "pub mod Core.Text.Regex.Generated;";
    let generated_source = "pub i32 ParseDigit() { return 1; }";
    let sources = [
        (&main_path, main_source),
        (&parser_path, parser_source),
        (&result_path, result_source),
        (&private_path, private_source),
        (&regex_path, regex_source),
        (&generated_path, generated_source),
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units.clone()),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let result_unit = SourceUnitId::new(&db, result_path);
    let generated_unit = SourceUnitId::new(&db, generated_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(18);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&units[0].program, generation);
    let result_index = SyntaxIndex::from_program(&units[2].program, generation);
    let generated_index = SyntaxIndex::from_program(&units[5].program, generation);

    let is_ok = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.IsOk").expect("public re-export"),
    );
    assert_eq!(
        resolved_item(&db, is_ok).expect("public re-export"),
        Some(beskid_queries::ResolvedItem {
            declaration: key(result_unit, generation, &result_index, NodeKind::FunctionDefinition, 0),
        })
    );

    let hidden = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.Private.Hidden").expect("private module"),
    );
    assert_eq!(resolved_item(&db, hidden).expect("private module"), None);

    let private_terminal = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.PrivateTerminal").expect("private terminal function"),
    );
    assert_eq!(resolved_item(&db, private_terminal).expect("private terminal function"), None);

    let second_alias = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Parser.Second.IsOk").expect("second public alias"),
    );
    assert_eq!(
        resolved_item(&db, second_alias).expect("second public alias"),
        Some(beskid_queries::ResolvedItem {
            declaration: key(result_unit, generation, &result_index, NodeKind::FunctionDefinition, 0,),
        })
    );

    let parse_digit = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Regex.Generated.ParseDigit").expect("generated module member"),
    );

    let fully_qualified = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::PathExpression,
        main_source.find("Core.Text.Regex.Generated.ParseDigit").expect("unbound fully-qualified module"),
    );
    assert_eq!(resolved_item(&db, fully_qualified).expect("unbound fully-qualified module"), None);
    assert_eq!(
        resolved_item(&db, parse_digit).expect("declared module member"),
        Some(beskid_queries::ResolvedItem {
            declaration: key(generated_unit, generation, &generated_index, NodeKind::FunctionDefinition, 0,),
        })
    );

    let constructor = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::EnumConstructorExpression,
        main_source.find("Parser.TextParseResult").expect("re-exported type"),
    );
    assert!(enum_constructor(&db, constructor).expect("re-exported type").is_some());

    let hidden_constructor = key_at_start(
        main_unit,
        generation,
        &main_index,
        NodeKind::EnumConstructorExpression,
        main_source.find("Parser.HiddenType").expect("private terminal enum"),
    );
    assert_unavailable(enum_constructor(&db, hidden_constructor));

    let hidden_type = key(main_unit, generation, &main_index, NodeKind::LetStatement, 0);
    assert_unavailable(beskid_queries::abi_type(&db, hidden_type));
}
