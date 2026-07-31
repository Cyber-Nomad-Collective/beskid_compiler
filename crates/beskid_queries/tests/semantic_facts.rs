use std::path::PathBuf;
use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, canonical_corelib_service_source_path,
    canonical_corelib_syscall_service_capability, canonical_corelib_syscall_sources,
};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex, SyntaxSnapshot};
use beskid_queries::{
    AggregateFieldShape, AstNodeKey, BeskidDatabase, CaptureStorageClass, ClosureAllocationStatus, ClosureCallTarget,
    ClosureCapture, ClosureEnvironmentField, ClosureLoweringStatus, ClosurePointerMapRequirement, CompletionContext,
    EnumLayoutFact, EnumMatchArmFact, EnumMatchFact, EnumVariantLayoutFact, ItemSignature, LocalSlot,
    MutableLocalAssignment, OperatorFact, ProjectSession, ScalarMatchArmFact, ScalarMatchFact, SemanticError,
    SemanticTypeId, SourceUnitId, SpawnDiagnosticKind, SpawnEntryValidation, SyntaxGenerationId, abi_type,
    aggregate_field_access, aggregate_layout, build_canonical_corelib_syscall_typed_program, build_typed_program,
    build_typed_program_with_corelib_syscall_services, call_abi_signature, call_argument_abi_type, call_arguments,
    call_lowering, callable_signature, capture_storage, cast_intents, child_nodes, closure_call_target,
    closure_environment, closure_signature, completion_candidates, constant_integer, control_flow, direct_callees,
    enum_constructor, enum_layout, enum_match, for_iterator_fact, generic_call_instantiation,
    generic_call_specialization, item_abi_signature, item_body, item_signature, literal_fact, local_slot,
    mutable_local_assignment, node_kind, node_span, node_type, nominal_member_receiver, operator_fact,
    primitive_numeric_conversion, reachable_items, resolved_item, resolved_local, runtime_intrinsic, scalar_match,
    spawn_entry_validation, spawn_legality, spawn_target, test_item,
};

fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    let error = match result {
        Ok(_) => panic!("current unported semantic query must fail explicitly"),
        Err(error) => error,
    };
    assert!(error.is_unavailable(), "{error:?}");
}

fn setup(source: &str) -> (BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, SyntaxIndex) {
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Main.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(3);
    let expanded = expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let index = SyntaxIndex::from_program(&expanded, generation);
    db.ensure_file_text(unit.path(&db).clone(), source.to_string());
    db.ensure_syntax_unit(project, unit, generation).expect("expanded syntax registration");
    (db, project, unit, generation, index)
}

#[test]
fn warm_point_query_uses_registered_expanded_syntax_without_reparse() {
    let (mut db, _project, unit, generation, index) = setup("i32 Main() { return 7; }");
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    assert!(literal_fact(&db, literal).expect("cold literal").is_some());
    assert_eq!(node_type(&db, literal).expect("cold type"), Some(beskid_queries::SemanticTypeId::I32));

    db.ensure_file_text(unit.path(&db).clone(), "this is deliberately invalid Beskid source".to_string());
    assert!(literal_fact(&db, literal).expect("warm literal").is_some());
    assert_eq!(node_type(&db, literal).expect("warm type"), Some(beskid_queries::SemanticTypeId::I32));
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

#[test]
fn module_hexadecimal_integer_constant_has_an_immediate_fact() {
    let (db, _project, unit, generation, index) =
        setup("const FIBER_NONE = 0xFFFF;\nword Main() { return FIBER_NONE; }");
    let constant_path = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(constant_integer(&db, constant_path).expect("constant fact"), Some(0xFFFF));
}

#[test]
fn primitive_numeric_conversion_call_has_a_typed_result_without_dynamic_dispatch() {
    let (db, _project, unit, generation, index) = setup("i64 Main(word index) { return i64(index); }");
    let conversion = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(node_type(&db, conversion).expect("conversion type"), Some(SemanticTypeId::I64));
    assert_eq!(abi_type(&db, conversion).expect("conversion ABI type"), Some(SemanticTypeId::I64));
    assert_eq!(
        primitive_numeric_conversion(&db, conversion).expect("conversion fact"),
        Some(beskid_queries::PrimitiveNumericConversion { from: SemanticTypeId::WORD, to: SemanticTypeId::I64 })
    );
}

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
    assert_eq!(
        resolved_item(&db, fully_qualified).expect("unbound fully-qualified module"),
        Some(beskid_queries::ResolvedItem {
            declaration: key(generated_unit, generation, &generated_index, NodeKind::FunctionDefinition, 0),
        })
    );
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

#[test]
fn aggregate_layout_keeps_channel_options_nominal_capacity() {
    let source = "enum ChannelCapacity { Unbounded(), Bounded(i64 capacity) } type ChannelOptions { ChannelCapacity capacity, bool singleReader, bool singleWriter }";
    let (db, _project, unit, generation, index) = setup(source);
    let options = key(unit, generation, &index, NodeKind::TypeDefinition, 0);
    let capacity = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let layout = aggregate_layout(&db, options).expect("layout query").expect("layout");
    assert_eq!(layout.fields.len(), 3);
    assert_eq!(layout.fields[0].0.as_ref(), "capacity");
    assert_eq!(layout.fields[0].1, AggregateFieldShape::Nominal(capacity));
    assert_eq!(layout.fields[1].1, AggregateFieldShape::Scalar(SemanticTypeId::BOOL));
}

#[test]
fn event_bearing_aggregate_keeps_value_field_layout_and_projection_for_cyb_162() {
    let source = r#"
type ProgressBar {
    i64 percent,
    i32 anchorRow,
    event{4} onTick(),
}
bool Main(ProgressBar bar) { return bar.anchorRow == 1; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let declaration = key(unit, generation, &index, NodeKind::TypeDefinition, 0);
    let projection = key(unit, generation, &index, NodeKind::PathExpression, 0);

    let layout = aggregate_layout(&db, declaration)
        .expect("event-bearing aggregate layout query")
        .expect("event-bearing aggregate layout");
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].0.as_ref(), "percent");
    assert_eq!(layout.fields[1].0.as_ref(), "anchorRow");
    assert_eq!(
        aggregate_field_access(&db, projection)
            .expect("event-bearing aggregate projection query")
            .expect("event-bearing aggregate projection")
            .index,
        1
    );
}

#[test]
fn generic_aggregate_direct_field_projection_uses_the_explicit_receiver_application_for_cyb_140() {
    let source = r#"
type ProgressBar<T> { T percent }
unit Equal<T>(T actual, T expected) { return; }
unit Main() {
    ProgressBar<i64> bar = ProgressBar<i64> { percent: 100_i64 };
    ProgressBar<i64> low = ProgressBar<i64> { percent: 0_i64 };
    Equal<i64>(bar.percent, low.percent);
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let arguments =
        call_arguments(&db, call).expect("generic direct-field arguments").expect("generic direct-field arguments");

    assert_eq!(abi_type(&db, arguments[0]), Ok(Some(SemanticTypeId::I64)));
    assert_eq!(abi_type(&db, arguments[1]), Ok(Some(SemanticTypeId::I64)));
    assert_eq!(
        generic_call_specialization(&db, call).expect("generic direct-field specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: key(unit, generation, &index, NodeKind::FunctionDefinition, 0),
            arguments: Arc::from([SemanticTypeId::I64]),
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64]),
                result: SemanticTypeId::UNIT,
            },
        })
    );
}

#[test]
fn aggregate_field_projection_abi_remains_closed_for_inferred_and_chained_receivers_for_cyb_140() {
    let inferred = r#"
type ProgressBar<T> { T percent }
unit Main() {
    let bar = ProgressBar<i64> { percent: 100_i64 };
    bar.percent;
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(inferred);
    let projection = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        inferred.find("bar.percent").expect("inferred projection"),
    );
    assert_unavailable(abi_type(&db, projection));

    let chained = r#"
type Inner { i64 percent }
type Outer { Inner bar }
unit Main() {
    Outer outer = Outer { bar: Inner { percent: 100_i64 } };
    outer.bar.percent;
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(chained);
    let projection = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        chained.find("outer.bar.percent").expect("chained projection"),
    );
    assert_unavailable(abi_type(&db, projection));
}

#[test]
fn sample_mod_method_abi_signatures_include_pointer_receiver_and_nominal_parameter() {
    let source = include_str!("../../beskid_tests/fixtures/mods/sample_mod/Src/Mod.bd");
    let (db, _project, unit, generation, index) = setup(source);
    let methods = index
        .ids_of_kind(NodeKind::MethodDefinition)
        .map(|node| AstNodeKey { unit, generation, node })
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 5);
    for method in methods {
        assert_eq!(
            item_abi_signature(&db, method).expect("method ABI signature"),
            Some(ItemSignature {
                parameters: Arc::from([SemanticTypeId::POINTER, SemanticTypeId::POINTER]),
                result: SemanticTypeId::POINTER,
            }),
        );
    }
}

#[test]
fn enum_layout_keeps_channel_capacity_variants_in_source_order() {
    let source = "enum ChannelCapacity { Unbounded(), Bounded(i64 capacity) } type ChannelOptions { ChannelCapacity capacity, bool singleReader, bool singleWriter }";
    let (db, _project, unit, generation, index) = setup(source);
    let capacity = key(unit, generation, &index, NodeKind::EnumDefinition, 0);

    let layout = enum_layout(&db, capacity).expect("layout query").expect("layout");
    assert_eq!(
        layout,
        EnumLayoutFact {
            variants: Arc::from([
                EnumVariantLayoutFact { name: Arc::from("Unbounded"), fields: Arc::from([]) },
                EnumVariantLayoutFact {
                    name: Arc::from("Bounded"),
                    fields: Arc::from([(Arc::from("capacity"), AggregateFieldShape::Scalar(SemanticTypeId::I64),)]),
                },
            ]),
        }
    );
}

#[test]
fn enum_layout_instantiates_concrete_generic_result_payloads() {
    let source = "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(1); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let syscall_error = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let result = key(unit, generation, &index, NodeKind::EnumDefinition, 1);
    let constructor = key(unit, generation, &index, NodeKind::EnumConstructorExpression, 0);
    let payload = key(unit, generation, &index, NodeKind::LiteralExpression, 0);

    assert_unavailable(enum_layout(&db, result));
    assert_eq!(
        enum_layout(&db, constructor).expect("concrete generic layout query").expect("concrete generic layout"),
        EnumLayoutFact {
            variants: Arc::from([
                EnumVariantLayoutFact {
                    name: Arc::from("Ok"),
                    fields: Arc::from([(Arc::from("value"), AggregateFieldShape::Scalar(SemanticTypeId::I64),)]),
                },
                EnumVariantLayoutFact {
                    name: Arc::from("Error"),
                    fields: Arc::from([(Arc::from("error"), AggregateFieldShape::Nominal(syscall_error),)]),
                },
            ]),
        }
    );
    assert_eq!(
        enum_constructor(&db, constructor).expect("concrete generic constructor query"),
        Some(beskid_queries::EnumConstructorFact {
            declaration: result,
            variant_index: 0,
            payloads: Arc::from([payload]),
        }),
    );
}

#[test]
fn enum_layout_rejects_inexact_generic_applications() {
    let cases = [
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64>::Ok(1); return 0; }",
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<Missing, SyscallError>::Ok(1); return 0; }",
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64(i64), SyscallError>::Ok(1); return 0; }",
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Outer<i64>.Result<i64, SyscallError>::Ok(1); return 0; }",
    ];
    for source in cases {
        let (db, _project, unit, generation, index) = setup(source);
        let constructor = key(unit, generation, &index, NodeKind::EnumConstructorExpression, 0);
        assert_unavailable(enum_layout(&db, constructor));
        assert_unavailable(enum_constructor(&db, constructor));
    }
}

#[test]
fn generic_enum_match_uses_the_explicit_scrutinee_application_for_cyb_137() {
    let source = "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64, string> value = Result<i64, string>::Ok(1); return match value { Result::Ok(_) => 1, Result::Error(_) => 0, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    let fact =
        enum_match(&db, expression).expect("generic enum match query").expect("explicit generic enum scrutinee match");
    assert_eq!(fact.declaration, key(unit, generation, &index, NodeKind::EnumDefinition, 0));
    assert_eq!(fact.arms.len(), 2);
    assert_eq!(fact.arms[0].variant_index, Some(0));
    assert_eq!(fact.arms[1].variant_index, Some(1));
    assert_eq!(fact.layout.variants.len(), 2);
    assert_eq!(
        fact.layout.variants[0].fields.as_ref(),
        &[(Arc::from("value"), beskid_queries::AggregateFieldShape::Scalar(SemanticTypeId::I64))]
    );
    assert_eq!(
        fact.layout.variants[1].fields.as_ref(),
        &[(Arc::from("error"), beskid_queries::AggregateFieldShape::Scalar(SemanticTypeId::STRING))]
    );
}

#[test]
fn imported_generic_enum_match_preserves_the_qualified_scrutinee_provenance_for_cyb_140() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-enum-match/project/src");
    let output_path = root.join("Core/Output/Output.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let error_path = root.join("Core/Syscall/SyscallError.bd");
    let output_source = r#"
use Core.Results;
use Core.Syscall.SyscallError;
unit Write() {
    Core.Results.Result<i64, SyscallError> result = Core.Results.Result<i64, SyscallError>::Ok(1_i64);
    match result {
        Result::Ok(_) => {},
        Result::Error(_) => {},
    };
    return;
}
"#;
    let results_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let error_source = "pub enum SyscallError { InvalidFd(i64 fd) }";
    let sources = [(&output_path, output_source), (&results_path, results_source), (&error_path, error_source)];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let output_program = units[0].program.clone();
    let results_program = units[1].program.clone();
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let output_unit = SourceUnitId::new(&db, output_path);
    let results_unit = SourceUnitId::new(&db, results_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        output_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(98);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let output_index = SyntaxIndex::from_program(&output_program, generation);
    let results_index = SyntaxIndex::from_program(&results_program, generation);
    let expression = key(output_unit, generation, &output_index, NodeKind::MatchExpression, 0);
    let constructor = key(output_unit, generation, &output_index, NodeKind::EnumConstructorExpression, 0);

    assert!(
        enum_layout(&db, constructor).expect("qualified imported Result constructor layout").is_some(),
        "the imported generic Result application must retain its concrete layout"
    );

    let fact = enum_match(&db, expression)
        .expect("imported generic enum match query")
        .expect("qualified imported Result match fact");
    assert_eq!(fact.declaration, key(results_unit, generation, &results_index, NodeKind::EnumDefinition, 0,));
    assert_eq!(fact.arms.len(), 2);
    assert_eq!(fact.arms[0].variant_index, Some(0));
    assert_eq!(fact.arms[1].variant_index, Some(1));
}

#[test]
fn imported_generic_enum_match_accepts_fully_qualified_one_type_per_file_terror_for_cyb_137() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-enum-match-qualified-terror/project/src");
    let main_path = root.join("Main.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let error_path = root.join("Core/Syscall/SyscallError.bd");
    let main_source = r#"
use Core.Results;
unit Main() {
    Core.Results.Result<i64, Core.Syscall.SyscallError> result =
        Core.Results.Result<i64, Core.Syscall.SyscallError>::Ok(1_i64);
    match result {
        Result::Ok(_) => {},
        Result::Error(_) => {},
    };
    return;
}
"#;
    let results_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let error_source = "pub enum SyscallError { InvalidFd(i64 fd) }";
    let sources = [(&main_path, main_source), (&results_path, results_source), (&error_path, error_source)];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: (*path).clone(),
            source: (*source).to_string(),
            program: expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH),
        })
        .collect::<Vec<_>>();
    let main_program = units[0].program.clone();
    let results_program = units[1].program.clone();
    let error_program = units[2].program.clone();
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let results_unit = SourceUnitId::new(&db, results_path);
    let error_unit = SourceUnitId::new(&db, error_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(137);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let results_index = SyntaxIndex::from_program(&results_program, generation);
    let error_index = SyntaxIndex::from_program(&error_program, generation);
    let expression = key(main_unit, generation, &main_index, NodeKind::MatchExpression, 0);

    let fact = enum_match(&db, expression)
        .expect("qualified TError enum match query")
        .expect("Core.Syscall.SyscallError type-arg must yield enum_match facts (CYB-137)");
    assert_eq!(fact.declaration, key(results_unit, generation, &results_index, NodeKind::EnumDefinition, 0,));
    assert_eq!(fact.arms.len(), 2);
    assert_eq!(
        fact.layout.variants[1].fields.as_ref(),
        &[(
            Arc::from("error"),
            beskid_queries::AggregateFieldShape::Nominal(key(
                error_unit,
                generation,
                &error_index,
                NodeKind::EnumDefinition,
                0,
            )),
        )]
    );
}

#[test]
fn enum_constructor_selects_the_source_variant_and_single_payload() {
    let source = "enum Choice { None(), Some(i32 value) } i32 Main() { Choice choice = Choice::Some(7); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let constructor = key(unit, generation, &index, NodeKind::EnumConstructorExpression, 0);
    let declaration = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let payload = key(unit, generation, &index, NodeKind::LiteralExpression, 0);

    assert_eq!(
        enum_layout(&db, constructor).expect("constructor layout query").expect("constructor layout").variants.len(),
        2,
    );

    assert_eq!(
        enum_constructor(&db, constructor).expect("enum constructor query"),
        Some(beskid_queries::EnumConstructorFact { declaration, variant_index: 1, payloads: Arc::from([payload]) })
    );
}

#[test]
fn enum_constructor_preserves_multiple_payloads_in_source_order() {
    let source = "enum Pair { Value(i32 left, i32 right) } i32 Main() { Pair pair = Pair::Value(1, 2); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let constructor = key(unit, generation, &index, NodeKind::EnumConstructorExpression, 0);
    let declaration = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let left = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let right = key(unit, generation, &index, NodeKind::LiteralExpression, 1);

    assert_eq!(
        enum_constructor(&db, constructor).expect("multi-payload enum constructor"),
        Some(beskid_queries::EnumConstructorFact { declaration, variant_index: 0, payloads: Arc::from([left, right]) })
    );
}

#[test]
fn enum_match_keeps_source_ordered_nullary_variant_arms() {
    let source = "enum Choice { None(), Some() } i32 Main() { return match Choice::Some() { Choice::None() => 1, Choice::Some() => 2, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let declaration = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let first_body = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let second_body = key(unit, generation, &index, NodeKind::LiteralExpression, 1);

    assert_eq!(
        enum_match(&db, expression).expect("enum match query"),
        Some(EnumMatchFact {
            declaration,
            layout: EnumLayoutFact {
                variants: Arc::from([
                    EnumVariantLayoutFact { name: Arc::from("None"), fields: Arc::from([]) },
                    EnumVariantLayoutFact { name: Arc::from("Some"), fields: Arc::from([]) },
                ]),
            },
            arms: Arc::from([
                EnumMatchArmFact { variant_index: Some(0), body: first_body, bindings: Arc::from([]) },
                EnumMatchArmFact { variant_index: Some(1), body: second_body, bindings: Arc::from([]) },
            ]),
        })
    );
}

#[test]
fn scalar_match_keeps_ordered_integer_literals_and_terminal_wildcard() {
    let source = r#"string Punctuation(u8 value) {
        return match value { 32 => " ", 33 => "!", _ => "" };
    }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let scrutinee = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let first_body = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let second_body = key(unit, generation, &index, NodeKind::LiteralExpression, 1);
    let wildcard_body = key(unit, generation, &index, NodeKind::LiteralExpression, 2);

    assert_eq!(
        scalar_match(&db, expression).expect("scalar match query"),
        Some(ScalarMatchFact {
            scrutinee,
            semantic_type: SemanticTypeId::U8,
            arms: Arc::from([
                ScalarMatchArmFact { discriminant: Some(32), body: first_body },
                ScalarMatchArmFact { discriminant: Some(33), body: second_body },
                ScalarMatchArmFact { discriminant: None, body: wildcard_body },
            ]),
        })
    );
}

#[test]
fn enum_match_accepts_enum_valued_aggregate_field_scrutinee() {
    let source = r#"
enum ChannelCapacity { Unbounded(), Bounded(i64 capacity) }
type ChannelOptions { ChannelCapacity capacity }
i64 EncodeCapacity(ChannelOptions options) {
    return match options.capacity {
        ChannelCapacity::Unbounded() => 0,
        ChannelCapacity::Bounded(capacity) => capacity,
    };
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let declaration = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let projection = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert!(aggregate_field_access(&db, projection).expect("aggregate field query").is_some());
    let fact = enum_match(&db, expression)
        .expect("aggregate field enum match query")
        .expect("enum-valued aggregate field match fact");
    assert_eq!(fact.declaration, declaration);
    assert_eq!(fact.arms.len(), 2);
    assert_eq!(fact.arms[0].variant_index, Some(0));
    assert_eq!(fact.arms[1].variant_index, Some(1));
}

fn key(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    occurrence: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .ids_of_kind(kind)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {kind:?} occurrence {occurrence}")),
    }
}

fn key_at_start(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    start: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .metadata()
            .iter()
            .find(|metadata| metadata.kind == kind && metadata.span.is_some_and(|span| span.start == start))
            .unwrap_or_else(|| panic!("missing {kind:?} at byte {start}"))
            .id,
    }
}

#[test]
fn completion_candidates_are_generation_safe_and_deterministic() {
    let source = "i32 Zebra() { return 0; } i32 Alpha() { return Zebra(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let cursor = source.find("Zebra();").expect("call");
    let candidates = completion_candidates(
        &db,
        program,
        CompletionContext { cursor, replacement_start: cursor, replacement_end: cursor + 1 },
    )
    .expect("completion")
    .expect("current generation");
    assert_eq!(candidates.iter().map(|candidate| candidate.label.as_ref()).collect::<Vec<_>>(), vec!["Zebra"]);
    assert_eq!((candidates[0].replacement_start, candidates[0].replacement_end), (cursor, cursor + 1));
    assert_eq!(
        completion_candidates(
            &db,
            AstNodeKey { generation: SyntaxGenerationId(generation.0 - 1), ..program },
            CompletionContext { cursor, replacement_start: cursor, replacement_end: cursor }
        ),
        Ok(None)
    );
    let unicode = "i32 Main() { return \"é\"; }";
    let (db, _project, unit, generation, index) = setup(unicode);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let invalid = unicode.find('é').expect("unicode") + 1;
    assert_eq!(
        completion_candidates(
            &db,
            program,
            CompletionContext { cursor: invalid, replacement_start: invalid, replacement_end: invalid }
        ),
        Ok(None)
    );
}

#[test]
fn completion_candidates_cover_lexical_type_and_receiver_families() {
    let source = r#"type Value { i32 raw
i32 Sum(i32 first) { return first + raw; }
}
i32 Helper() { return 1; }
i32 Main(Value value) {
    let amount = 2;
    return value.Su;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);

    let lexical_cursor = source.find("return value").expect("lexical site");
    let lexical = completion_candidates(
        &db,
        program,
        CompletionContext {
            cursor: lexical_cursor,
            replacement_start: lexical_cursor,
            replacement_end: lexical_cursor,
        },
    )
    .expect("lexical completion")
    .expect("current generation");
    let lexical_labels = lexical.iter().map(|candidate| (candidate.label.as_ref(), candidate.kind)).collect::<Vec<_>>();
    assert!(
        lexical_labels.contains(&("amount", beskid_queries::CompletionKind::Variable)),
        "expected lexical local amount, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("value", beskid_queries::CompletionKind::Variable)),
        "expected lexical parameter value, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("Value", beskid_queries::CompletionKind::Type)),
        "expected type candidate Value, got {lexical_labels:?}"
    );
    assert!(
        lexical_labels.contains(&("Helper", beskid_queries::CompletionKind::Function)),
        "expected function candidate Helper, got {lexical_labels:?}"
    );

    let receiver_cursor = source.find("value.Su").expect("receiver site") + "value.".len();
    let receiver = completion_candidates(
        &db,
        program,
        CompletionContext {
            cursor: receiver_cursor,
            replacement_start: receiver_cursor,
            replacement_end: receiver_cursor + "Su".len(),
        },
    )
    .expect("receiver completion")
    .expect("receiver candidates");
    assert_eq!(
        receiver.iter().map(|candidate| (candidate.label.as_ref(), candidate.kind)).collect::<Vec<_>>(),
        vec![("Sum", beskid_queries::CompletionKind::Method)]
    );

    let inferred = "type Value { i32 raw }\ni32 Main() { let value = 1; return value.x; }";
    let (db, _project, unit, generation, index) = setup(inferred);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let inferred_cursor = inferred.find("value.x").expect("inferred site") + "value.".len();
    assert_eq!(
        completion_candidates(
            &db,
            program,
            CompletionContext {
                cursor: inferred_cursor,
                replacement_start: inferred_cursor,
                replacement_end: inferred_cursor + 1,
            }
        ),
        Ok(None),
        "inferred or non-nominal receivers remain unavailable"
    );
}

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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(17);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(25);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(54);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(55);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(57);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(56);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(38);
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
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
    let generation = SyntaxGenerationId(18);
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

#[test]
fn generic_imported_static_call_resolves_to_its_exact_syntax_item() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-import/project/src");
    let main_path = root.join("Main.bd");
    let channel_path = root.join("Concurrency/Channel.bd");
    let main_source = "use Concurrency.Channel;\nunit Main() { Channel<i64>.Create(); }";
    let channel_source = "pub unit Create<T>() { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let channel_program =
        expand_program(parse_program(channel_source).expect("channel parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
                logical_name: channel_path.display().to_string(),
                path: channel_path.clone(),
                source: channel_source.to_string(),
                program: channel_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let channel_unit = SourceUnitId::new(&db, channel_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(19);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let channel_index = SyntaxIndex::from_program(&channel_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(channel_unit, generation, &channel_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("generic imported static call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn imported_generic_type_annotation_resolves_without_registry_reentrance() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/imported-generic-type/project/src");
    let main_path = root.join("Main.bd");
    let envelope_path = root.join("Messaging/Envelope.bd");
    let main_source =
        "use Messaging.Envelope;\nunit Main() { Envelope<i64> envelope = Envelope<i64> { value: 1 }; return; }";
    let envelope_source = "pub type Envelope<T> { i64 value }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let envelope_program =
        expand_program(parse_program(envelope_source).expect("envelope parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
                logical_name: envelope_path.display().to_string(),
                path: envelope_path.clone(),
                source: envelope_source.to_string(),
                program: envelope_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(20);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let local = key(main_unit, generation, &main_index, NodeKind::LetStatement, 0);

    assert_eq!(abi_type(&db, local).expect("imported generic type annotation ABI"), Some(SemanticTypeId::POINTER));
}

#[test]
fn imported_generic_nominal_calls_require_receiver_instantiation() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-nominal-receiver/project/src");
    let main_path = root.join("Main.bd");
    let hub_path = root.join("Collections/Hub.bd");
    let main_source =
        "use Collections.Hub;\nunit Main() { Hub.Create(); Hub<i64>.Create(1_i64); Hub.Create<i64>(1_i64); }";
    let hub_source = "type Hub<T> { i64 value }\npub unit Create<T>(T value) { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let hub_program = expand_program(parse_program(hub_source).expect("hub parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
                logical_name: hub_path.display().to_string(),
                path: hub_path.clone(),
                source: hub_source.to_string(),
                program: hub_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let hub_unit = SourceUnitId::new(&db, hub_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(55);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let hub_index = SyntaxIndex::from_program(&hub_program, generation);
    let declaration = key(hub_unit, generation, &hub_index, NodeKind::FunctionDefinition, 0);
    let missing_receiver = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let explicit_receiver = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);
    let method_generic = key(main_unit, generation, &main_index, NodeKind::CallExpression, 2);

    assert_unavailable(call_lowering(&db, missing_receiver));
    assert_unavailable(call_abi_signature(&db, missing_receiver));
    // An unavailable call site yields no call-derived ABI specialization. The query returns
    // `Ok(None)` (no specialization) rather than propagating the unavailable error, so reachable
    // Syscall/Output bodies with unresolved calls do not abort whole-module emission.
    assert_eq!(
        generic_call_specialization(&db, missing_receiver)
            .expect("missing receiver yields no specialization rather than an error"),
        None
    );
    assert_eq!(
        call_lowering(&db, explicit_receiver).expect("explicit receiver lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, explicit_receiver).expect("explicit receiver instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    assert_eq!(
        call_lowering(&db, method_generic).expect("method generic lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, method_generic).expect("method generic instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
}

#[test]
fn generic_imported_terminal_call_requires_an_exact_declared_generic_arity() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-terminal-import/project/src");
    let main_path = root.join("Main.bd");
    let channel_path = root.join("Concurrency/Channel.bd");
    let main_source = "use Concurrency.Channel;\nunit Main() { Channel.CreateWithOptions<i64>(); Channel.CreateWithOptions<i64, i32>(); }";
    let channel_source = "pub unit CreateWithOptions<T>() { return; }";
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let channel_program =
        expand_program(parse_program(channel_source).expect("channel parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
                logical_name: channel_path.display().to_string(),
                path: channel_path.clone(),
                source: channel_source.to_string(),
                program: channel_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let channel_unit = SourceUnitId::new(&db, channel_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(20);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let channel_index = SyntaxIndex::from_program(&channel_program, generation);
    let call = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let declaration = key(channel_unit, generation, &channel_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("generic imported terminal call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, call).expect("exact generic instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    let mismatched = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);
    assert_eq!(
        call_lowering(&db, mismatched).expect("mismatched generic terminal call"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
    assert_eq!(generic_call_instantiation(&db, mismatched).expect("mismatched generic instantiation"), None);
}

#[test]
fn generic_imported_receiver_call_uses_receiver_specialization_for_zero_argument_abi() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-receiver-import/project/src");
    let main_path = root.join("Main.bd");
    let hub_path = root.join("System/Hub.bd");
    let main_source = r#"
use System.Hub;
unit Main() {
    Hub<i64>.Create();
    Hub.Create();
}
"#;
    let hub_source = r#"
type Hub<T> { i64 value }
pub Hub<T> Create<T>() { return Hub<T> { value: 0_i64 }; }
"#;
    let main_program =
        expand_program(parse_program(main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let hub_program = expand_program(parse_program(hub_source).expect("hub parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
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
                logical_name: hub_path.display().to_string(),
                path: hub_path.clone(),
                source: hub_source.to_string(),
                program: hub_program.clone(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let hub_unit = SourceUnitId::new(&db, hub_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(22);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let hub_index = SyntaxIndex::from_program(&hub_program, generation);
    let declaration = key(hub_unit, generation, &hub_index, NodeKind::FunctionDefinition, 0);
    let receiver_specialized = key(main_unit, generation, &main_index, NodeKind::CallExpression, 0);
    let bare = key(main_unit, generation, &main_index, NodeKind::CallExpression, 1);

    assert_eq!(
        call_lowering(&db, receiver_specialized).expect("receiver specialization"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, receiver_specialized).expect("receiver specialization fact"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
            arguments: Arc::from([SemanticTypeId::I64]),
        })
    );
    assert_eq!(
        call_abi_signature(&db, receiver_specialized).expect("receiver call ABI"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        beskid_queries::abi_type(&db, receiver_specialized).expect("receiver call result ABI"),
        Some(SemanticTypeId::POINTER)
    );
    assert_unavailable(call_lowering(&db, bare));
}

#[test]
fn imported_homonymous_module_generic_envelope_retains_pointer_abi_specialization() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-envelope/project/src");
    let main_path = root.join("Main.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let error_path = root.join("Core/Syscall/SyscallError.bd");
    let syscall_path = root.join("Core/Syscall/Syscall.bd");
    let main_source = r#"
use Core.Results;
use Core.Syscall;
unit Main() {
    Core.Results.Result<i64, Core.Syscall.SyscallError> result = Core.Syscall.Write();
    Results.IsOk(result);
}
"#;
    let results_source = r#"
pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
pub bool IsOk<TValue, TError>(Result<TValue, TError> value) { return true; }
"#;
    let error_source = "pub enum SyscallError { InvalidFd(i64 fd) }";
    let syscall_source = r#"
use Core.Syscall.SyscallError;
pub Core.Results.Result<i64, Core.Syscall.SyscallError> Write() {
    return Result::Error(SyscallError::InvalidFd(1_i64));
}
"#;
    let sources = [
        (&main_path, main_source),
        (&results_path, results_source),
        (&error_path, error_source),
        (&syscall_path, syscall_source),
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
    let main_program = units[0].program.clone();
    let results_program = units[1].program.clone();
    let syscall_program = units[3].program.clone();
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let results_unit = SourceUnitId::new(&db, results_path);
    let syscall_unit = SourceUnitId::new(&db, syscall_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(23);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let results_index = SyntaxIndex::from_program(&results_program, generation);
    let syscall_index = SyntaxIndex::from_program(&syscall_program, generation);
    let declaration = key(results_unit, generation, &results_index, NodeKind::FunctionDefinition, 0);
    let write = key(syscall_unit, generation, &syscall_index, NodeKind::FunctionDefinition, 0);
    let call = main_index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: main_unit, generation, node })
        .find(|call| {
            generic_call_specialization(&db, *call)
                .ok()
                .flatten()
                .is_some_and(|specialization| specialization.declaration == declaration)
        })
        .expect("Results.IsOk call");
    let qualified_write = main_index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: main_unit, generation, node })
        .find(|call| {
            matches!(
                call_lowering(&db, *call).ok().flatten(),
                Some(beskid_queries::CallLowering::Direct(declaration)) if declaration == write
            )
        })
        .expect("qualified Core.Syscall.Write call");

    assert_eq!(
        item_abi_signature(&db, write).expect("Syscall.Write item ABI"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        call_lowering(&db, qualified_write).expect("qualified Core.Syscall.Write lowering"),
        Some(beskid_queries::CallLowering::Direct(write))
    );
    assert_eq!(
        call_lowering(&db, call).expect("qualified Results.IsOk lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        call_abi_signature(&db, call).expect("generic Results.IsOk call ABI"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL })
    );
    assert_eq!(
        generic_call_specialization(&db, call).expect("generic Results.IsOk specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration,
            arguments: Arc::from([SemanticTypeId::I64, SemanticTypeId::POINTER]),
            signature: ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL },
        })
    );
}

#[test]
fn canonical_core_error_qualified_write_has_a_direct_semantic_fact() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/core-error-qualified-call/project/src");
    let error_path = root.join("Core/Error/Error.bd");
    let results_path = root.join("Core/Results/Results.bd");
    let syscall_path = root.join("Core/Syscall/Syscall.bd");
    let syscall_error_path = root.join("Core/Syscall/SyscallError.bd");
    let descriptor_path = root.join("Core/Syscall/Descriptor.bd");
    let standard_stream_path = root.join("Core/Syscall/StandardStream.bd");
    let write_request_path = root.join("Core/Syscall/WriteRequest.bd");
    let error_source_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src/Core/Error/Error.bd");
    let error_source = std::fs::read_to_string(&error_source_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", error_source_path.display()));
    let sources = [
        (error_path.clone(), error_source),
        (
            results_path.clone(),
            "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }".into(),
        ),
        (
            syscall_path.clone(),
            "pub Core.Results.Result<i64, Core.Syscall.SyscallError> WriteWith(Core.Syscall.WriteRequest request) { return Result::Ok(0_i64); }".into(),
        ),
        (
            syscall_error_path,
            "pub enum SyscallError { IoFailure(i64 code) }".into(),
        ),
        (
            descriptor_path,
            "pub enum Descriptor { Standard(Core.Syscall.StandardStream stream), Raw(i64 fd) }".into(),
        ),
        (
            standard_stream_path,
            "pub enum StandardStream { Stdin, Stdout, Stderr }".into(),
        ),
        (
            write_request_path,
            "pub type WriteRequest { Core.Syscall.Descriptor descriptor, string data }".into(),
        ),
    ];
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            path: path.clone(),
            source: source.clone(),
            program: expand_program(
                parse_program(source).expect("parse Core.Error regression source"),
                DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            ),
        })
        .collect::<Vec<_>>();
    let error_program = units[0].program.clone();
    let syscall_program = units[2].program.clone();
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let error_unit = SourceUnitId::new(&db, error_path);
    let syscall_unit = SourceUnitId::new(&db, syscall_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        error_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(28);
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let error_index = SyntaxIndex::from_program(&error_program, generation);
    let syscall_index = SyntaxIndex::from_program(&syscall_program, generation);
    let call = key(error_unit, generation, &error_index, NodeKind::CallExpression, 0);
    let declaration = key(syscall_unit, generation, &syscall_index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        call_lowering(&db, call).expect("Core.Error WriteWith call lowering"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn generic_parameter_type_argument_call_remains_direct_inside_generic_body() {
    let source = r#"
type Channel<T> { i64 handle }
type Options { i64 flags }
Options Default() { return Options { flags: 0_i64 }; }
Channel<T> CreateWithOptions<T>(Options options) { return Channel<T> { handle: options.flags }; }
Channel<T> Create<T>() { return CreateWithOptions<T>(Default()); }
unit Main() { Channel<i64> ch = Create<i64>(); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let create_with_options = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let nested = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .find(|call| {
            matches!(
                call_lowering(&db, *call).ok().flatten(),
                Some(beskid_queries::CallLowering::Direct(declaration))
                    if declaration == create_with_options
            )
        })
        .expect("CreateWithOptions<T> call inside Create");

    assert_eq!(
        generic_call_instantiation(&db, nested).expect("parameter type-arg instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration: create_with_options,
            argument_count: 1,
            arguments: Arc::from([]),
        })
    );
    assert_eq!(
        call_abi_signature(&db, nested).expect("nested call ABI"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER })
    );
}

#[test]
fn inferred_generic_call_has_an_exact_argument_derived_abi_signature() {
    let source = r#"
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main() { Equal(1, 1, "because"); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let arguments = call_arguments(&db, call).expect("generic arguments").expect("generic arguments available");
    assert_eq!(abi_type(&db, arguments[0]), Ok(Some(SemanticTypeId::I32)));
    assert_eq!(abi_type(&db, arguments[1]), Ok(Some(SemanticTypeId::I32)));
    assert_eq!(abi_type(&db, arguments[2]), Ok(Some(SemanticTypeId::STRING)));

    assert_eq!(
        call_abi_signature(&db, call).expect("inferred generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I32, SemanticTypeId::I32, SemanticTypeId::STRING,]),
            result: SemanticTypeId::UNIT,
        })
    );
    assert_eq!(
        generic_call_specialization(&db, call).expect("inferred generic specialization"),
        Some(beskid_queries::GenericCallSpecialization {
            declaration: key(unit, generation, &index, NodeKind::FunctionDefinition, 0),
            arguments: Arc::from([SemanticTypeId::I32]),
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::I32, SemanticTypeId::I32, SemanticTypeId::STRING,]),
                result: SemanticTypeId::UNIT,
            },
        })
    );
}

#[test]
fn inferred_generic_call_allows_a_bare_integer_to_follow_an_exact_i64_argument() {
    let source = r#"
i64 Position() { return 0_i64; }
unit Equal<T>(T actual, T expected, string because) { return; }
unit Main() { Equal(Position(), 0, "initial position"); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_abi_signature(&db, call).expect("nested generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64, SemanticTypeId::STRING,]),
            result: SemanticTypeId::UNIT,
        })
    );
}

#[test]
fn explicit_generic_call_contextualizes_bare_integer_for_non_generic_i64_parameter() {
    let source = r#"
unit Register<T>(T receiver, i64 index, T value) { return; }
unit Main() { Register<i64>(1_i64, 0, 2_i64); return; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_abi_signature(&db, call).expect("explicit generic call signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64, SemanticTypeId::I64]),
            result: SemanticTypeId::UNIT,
        })
    );
}

#[test]
fn inferred_generic_call_does_not_rebind_an_explicit_integer_suffix() {
    for (bound_type, explicit_literal) in [("i64", "0_i32"), ("i32", "0_i64"), ("i32", "0_u8")] {
        let source = format!(
            r#"
{bound_type} Position() {{ return 0_{bound_type}; }}
unit Equal<T>(T actual, T expected, string because) {{ return; }}
unit Main() {{ Equal(Position(), {explicit_literal}, "initial position"); return; }}
"#
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

        assert_unavailable(call_abi_signature(&db, call));
    }
}

#[test]
fn nominal_generic_types_have_only_source_derived_pointer_abi_facts() {
    let source = r#"
type Channel<T> { i64 handle }
type Pair<T> { i64 left, i64 right }
Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; }
Pair<T> CreatePair<T>() { return Pair<T> { left: 0_i64, right: 0_i64 }; }
unit Main() {
    Channel<i64> channel = Create<i64>();
    Pair<i64> pair = CreatePair<i64>();
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let create = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let create_pair = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let channel_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let pair_call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let channel_let = key(unit, generation, &index, NodeKind::LetStatement, 0);
    let pair_let = key(unit, generation, &index, NodeKind::LetStatement, 1);

    assert_eq!(
        beskid_queries::item_abi_signature(&db, create).expect("generic item has no fixed ABI"),
        None,
        "generic factories must not publish a single item ABI; callers specialize"
    );
    assert_eq!(beskid_queries::abi_type(&db, channel_call).expect("nominal call ABI"), Some(SemanticTypeId::POINTER));
    assert_eq!(beskid_queries::abi_type(&db, channel_let).expect("nominal local ABI"), Some(SemanticTypeId::POINTER));
    assert_eq!(
        beskid_queries::item_abi_signature(&db, create_pair).expect("generic multi-field factory has no fixed ABI"),
        None
    );
    assert_eq!(
        beskid_queries::abi_type(&db, pair_call).expect("multi-field nominal call ABI"),
        Some(SemanticTypeId::POINTER)
    );
    assert_eq!(
        beskid_queries::abi_type(&db, pair_let).expect("multi-field nominal local ABI"),
        Some(SemanticTypeId::POINTER)
    );
}

#[test]
fn structural_facts_survive_while_unported_semantics_are_unavailable() {
    let source = r#"
i32 Helper(i64 value) { return 1; }
i32 Main() {
    let local = 2;
    Helper(local);
    return local;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);

    let helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let local_reference = key(unit, generation, &index, NodeKind::PathExpression, 2);
    let integer = key(unit, generation, &index, NodeKind::Literal, 0);
    let item_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(node_kind(&db, main).expect("kind"), Some(NodeKind::FunctionDefinition));
    assert!(node_span(&db, main).expect("span").is_some());
    assert!(
        child_nodes(&db, main)
            .expect("children")
            .unwrap()
            .iter()
            .all(|child| child.unit == unit && child.generation == generation)
    );
    assert!(literal_fact(&db, integer).expect("literal").is_some());
    assert!(item_body(&db, main).expect("body").is_some());

    assert_eq!(node_type(&db, integer).expect("integer type"), Some(beskid_queries::SemanticTypeId::I32));
    assert_eq!(
        item_signature(&db, helper).expect("helper signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I64].into(),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        item_signature(&db, main).expect("main signature"),
        Some(beskid_queries::ItemSignature { parameters: Arc::from([]), result: beskid_queries::SemanticTypeId::I32 })
    );
    assert_eq!(
        control_flow(&db, main).expect("control flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: false })
    );
    assert!(resolved_local(&db, local_reference).expect("local resolution").is_some());
    assert_eq!(
        resolved_item(&db, item_reference).expect("item resolution"),
        Some(beskid_queries::ResolvedItem { declaration: helper })
    );
    assert_eq!(call_lowering(&db, call).expect("call lowering"), Some(beskid_queries::CallLowering::Direct(helper)));
    assert_unavailable(cast_intents(&db, call));
    assert_eq!(direct_callees(&db, main).expect("direct callees"), Some(Arc::from([helper])));
    assert_eq!(reachable_items(&db, program, main).expect("reachable items"), Some(Arc::from([main, helper])));
    assert_unavailable(runtime_intrinsic(&db, call));
}

#[test]
fn node_type_derives_primitive_literals_and_annotated_local_references() {
    let source = r#"unit Main(i64 input) {
    i64 local = input;
    let flag = true;
    let ratio = 1.5;
    let text = "text";
    let letter = 'x';
    let byte = 1_u8;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let input_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert_eq!(node_type(&db, input_reference).expect("input type"), Some(beskid_queries::SemanticTypeId::I64));
    let expected = [
        beskid_queries::SemanticTypeId::BOOL,
        beskid_queries::SemanticTypeId::F64,
        beskid_queries::SemanticTypeId::STRING,
        beskid_queries::SemanticTypeId::CHAR,
        beskid_queries::SemanticTypeId::U8,
    ];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let literal = key(unit, generation, &index, NodeKind::Literal, occurrence);
        assert_eq!(node_type(&db, literal).expect("literal type"), Some(expected));
    }
}

#[test]
fn node_type_does_not_guess_complex_local_types() {
    let source = "Value Identity(Value input) { return input; }";
    let (db, _project, unit, generation, index) = setup(source);
    let input = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert_unavailable(node_type(&db, input));
}

#[test]
fn node_type_uses_the_exact_scalar_enum_payload_binding_shape() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(value) => value, Result::Error(error) => error, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let value = key(unit, generation, &index, NodeKind::PathExpression, 1);
    let error = key(unit, generation, &index, NodeKind::PathExpression, 2);

    assert_eq!(node_type(&db, value).expect("Ok binding type"), Some(SemanticTypeId::I64));
    assert_eq!(node_type(&db, error).expect("Error binding type"), Some(SemanticTypeId::I64));
}

#[test]
fn node_type_uses_the_exact_nominal_enum_payload_binding_shape() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } unit Main(Descriptor descriptor) { match descriptor { Descriptor::Standard(stream) => { stream; }, Descriptor::Raw(_) => {}, }; return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let stream = key(unit, generation, &index, NodeKind::PathExpression, 1);

    assert_eq!(node_type(&db, stream).expect("StandardStream binding type"), Some(SemanticTypeId::POINTER));
}

#[test]
fn enum_match_uses_the_exact_nominal_pattern_binding_layout() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } i64 Main(Descriptor descriptor) { return match descriptor { Descriptor::Standard(stream) => match stream { StandardStream::Stdin => 0_i64, StandardStream::Stdout => 1_i64, StandardStream::Stderr => 2_i64, }, Descriptor::Raw(fd) => fd, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let inner_match = key(unit, generation, &index, NodeKind::MatchExpression, 1);

    assert!(
        enum_match(&db, inner_match).expect("inner match query").is_some(),
        "a direct nominal payload binding must supply the inner enum layout"
    );
}

#[test]
fn node_type_composes_enum_match_results_from_binding_aware_arm_nodes() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } i64 Main(Descriptor descriptor) { return match descriptor { Descriptor::Standard(stream) => match stream { StandardStream::Stdin => 0_i64, StandardStream::Stdout => 1_i64, StandardStream::Stderr => 2_i64, }, Descriptor::Raw(fd) => fd, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let outer_match = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_eq!(node_type(&db, outer_match).expect("outer match type"), Some(SemanticTypeId::I64));
}

#[test]
fn node_type_rejects_enum_match_results_with_mixed_arm_types() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } unit Main(Result result) { match result { Result::Ok(value) => value, Result::Error(error) => true, }; return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let outer_match = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_unavailable(node_type(&db, outer_match));
}

#[test]
fn node_type_uses_an_exact_direct_call_abi_result() {
    let source = "i64 Fd() { return 0_i64; } i64 Main() { return Fd(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(node_type(&db, call).expect("direct call type"), Some(SemanticTypeId::I64));
}

#[test]
fn literal_enum_payload_pattern_remains_unavailable_to_type_queries() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(7_i64) => 1_i64, Result::Error(_) => 0_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_unavailable(enum_match(&db, expression));
}

#[test]
fn cast_intents_use_exact_typed_let_constraints_and_local_types() {
    let source = r#"unit Main() {
    i64 widenedLiteral = 1;
    i32 source = 2;
    i64 widenedLocal = source;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    let source_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let expected = Arc::from([beskid_queries::CastIntent {
        from: beskid_queries::SemanticTypeId::I32,
        to: beskid_queries::SemanticTypeId::I64,
    }]);
    assert_eq!(cast_intents(&db, literal).expect("literal cast"), Some(Arc::clone(&expected)));
    assert_eq!(cast_intents(&db, source_reference).expect("local cast"), Some(expected));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "unit Main() { i64 replacement = 1_i64; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(cast_intents(&db, literal).expect("stale cast"), None);
}

#[test]
fn cast_intents_use_manifest_runtime_parameter_types_for_contextual_literals() {
    let source = "pointer Main(pointer state) { return pointer_add(state, 8); }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("runtime argument cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn cast_intents_use_binary_operand_types_for_contextual_literals() {
    let source = "bool Main(word size) { return size < 16; }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("binary operand cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn cast_intents_keep_nested_call_literals_bound_to_the_parameter_type() {
    let source = r#"
pointer NativePointer(word value) { return value; }
bool Main(pointer object) { return object == NativePointer(0); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("nested call argument cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn item_signatures_cover_primitive_functions_methods_and_contracts() {
    let source = r#"
i64 Convert(i32 value, bool checked) { return value; }
type Counter { i64 value }
impl Counter { bool IsPositive(u8 threshold) { return true; } }
contract Converter { string Format(char value); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let contract = key(unit, generation, &index, NodeKind::ContractMethodSignature, 0);

    assert_eq!(
        item_signature(&db, function).expect("function signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I32, beskid_queries::SemanticTypeId::BOOL,].into(),
            result: beskid_queries::SemanticTypeId::I64,
        })
    );
    assert_eq!(
        item_signature(&db, method).expect("method signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::U8].into(),
            result: beskid_queries::SemanticTypeId::BOOL,
        })
    );
    assert_eq!(
        item_signature(&db, contract).expect("contract signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::CHAR].into(),
            result: beskid_queries::SemanticTypeId::STRING,
        })
    );
}

#[test]
fn test_items_have_a_unit_signature_and_own_generation_safe_body_cursor() {
    let source = "test Smoke { return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let test = key(unit, generation, &index, NodeKind::TestDefinition, 0);

    assert_eq!(
        item_signature(&db, test).expect("test signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT })
    );
    assert_eq!(item_body(&db, test).expect("test body"), Some(test));
}

#[test]
fn test_item_facts_preserve_metadata_and_reject_stale_generations() {
    let source = r#"test Smoke {
        meta { group = "fast"; tags = "unit, smoke"; }
        skip { condition = true; reason = "not on this host"; }
        return;
    }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let test = key(unit, generation, &index, NodeKind::TestDefinition, 0);

    let facts = test_item(&db, test).expect("test facts query").expect("current test facts");
    assert_eq!(facts.name.as_ref(), "Smoke");
    assert_eq!(facts.qualified_name.as_ref(), "Smoke");
    assert_eq!(facts.group.as_deref(), Some("fast"));
    assert_eq!(facts.tags.iter().map(|tag| tag.as_ref()).collect::<Vec<_>>(), ["unit", "smoke"]);
    assert_eq!(facts.skip_condition, Some(true));
    assert_eq!(facts.skip_reason.as_deref(), Some("not on this host"));
    assert_eq!(
        test_item(&db, AstNodeKey { generation: SyntaxGenerationId(generation.0 - 1), ..test })
            .expect("stale test facts"),
        None
    );
}

#[test]
fn item_signature_does_not_guess_complex_type_identity() {
    let source = "Value Identity(Value value) { return value; }";
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert_unavailable(item_signature(&db, function));
}

#[test]
fn call_lowering_classifies_immediate_lambda_without_name_resolution() {
    let source = "i64 Main() { return ((i64 value) => value)(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(call_lowering(&db, call).expect("lambda call lowering"), Some(beskid_queries::CallLowering::Dynamic));
}

#[test]
fn call_arguments_preserve_exact_root_keys_and_source_order() {
    let source = r#"i32 Target(i32 first, i32 second, i32 third) { return first; }
i32 Main() {
    let value = 4;
    Target(1, Target(2, 3, 4), value);
    Target();
    return 0;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let outer_call_offset = source.find("Target(1").expect("outer call");
    let nested_call_offset = source.find("Target(2").expect("nested call");
    let empty_call_offset = source.find("Target();").expect("empty call");
    let outer_call =
        key_at_start(unit, generation, &index, NodeKind::CallExpression, outer_call_offset + "Target".len());
    let empty_call =
        key_at_start(unit, generation, &index, NodeKind::CallExpression, empty_call_offset + "Target".len());
    let expected = [
        key_at_start(unit, generation, &index, NodeKind::Expression, source.find("1,").expect("first argument")),
        key_at_start(unit, generation, &index, NodeKind::Expression, nested_call_offset),
        key_at_start(unit, generation, &index, NodeKind::Expression, source.find("value);").expect("value argument")),
    ];

    assert_eq!(call_arguments(&db, outer_call).expect("outer arguments"), Some(Arc::from(expected)));
    assert_eq!(call_arguments(&db, empty_call).expect("empty arguments"), Some(Arc::from([])));
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_eq!(call_arguments(&db, main).expect("non-call"), None);

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(call_arguments(&db, outer_call).expect("stale arguments"), None);
}

#[test]
fn negative_integer_call_argument_inherits_the_parameter_abi() {
    let source = "i64 Identity(i64 value) { return value; } i64 Main() { return Identity(-5); }";
    let (db, _project, unit, generation, index) = setup(source);
    let unary = key(unit, generation, &index, NodeKind::UnaryExpression, 0);

    assert_eq!(call_argument_abi_type(&db, unary).expect("negative argument ABI"), Some(SemanticTypeId::I64));
}

#[test]
fn explicit_generic_array_result_has_pointer_abi_specialization() {
    let source = r#"
T[] Empty<T>() { return __array_new(8, 0); }
unit Main() { i64[] values = Empty<i64>(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let binding = key(unit, generation, &index, NodeKind::LetStatement, 0);
    let expected = ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER };

    assert_eq!(call_abi_signature(&db, call).expect("array call ABI"), Some(expected.clone()));
    assert_eq!(
        generic_call_specialization(&db, call).expect("array specialization").map(|fact| fact.signature),
        Some(expected)
    );
    assert_eq!(abi_type(&db, binding).expect("array local ABI"), Some(SemanticTypeId::POINTER));
}

#[test]
fn call_lowering_resolves_named_targets() {
    let source = r#"
i64 Helper() { return 1; }
i64 Main() { return Helper(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let named_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(call_lowering(&db, named_call).expect("named call"), Some(beskid_queries::CallLowering::Direct(helper)));
}

#[test]
fn call_lowering_resolves_an_explicit_nominal_parameter_method() {
    let source = r#"
type Point { i32 x, i32 Ping() { return 7; } }
i32 Main(Point point) { return point.Ping(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let receiver = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("point) {").expect("parameter declaration"),
    );

    assert_eq!(
        call_lowering(&db, call).expect("nominal member call lowering"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(direct_callees(&db, main).expect("nominal member call graph"), Some(Arc::from([method])));
    assert_eq!(nominal_member_receiver(&db, receiver).expect("nominal receiver fact"), Some(declaration));
    assert_eq!(call_arguments(&db, call).expect("nominal member call arguments"), Some(Arc::from([receiver])));
}

#[test]
fn call_lowering_resolves_an_explicit_nominal_let_method() {
    let source = r#"
type Point { i32 x, i32 Ping() { return 7; } }
i32 Main() { Point point = Point { x: 1 }; return point.Ping(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let receiver = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("nominal let member call lowering"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(
        nominal_member_receiver(&db, receiver).expect("nominal let receiver fact"),
        Some(key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            source.find("point =").expect("let declaration"),
        ))
    );
}

#[test]
fn item_and_call_graph_facts_resolve_named_calls_and_recursion() {
    let source = r#"i32 Leaf() { return 1; }
i32 Recur(i32 count) {
    if count == 0 { return 0; }
    return Recur(count - 1);
}
i32 Main() {
    Leaf();
    return Recur(1);
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let leaf = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let recur = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    let leaf_call_offset = source.find("Leaf();").expect("leaf call");
    let leaf_path = key_at_start(unit, generation, &index, NodeKind::PathExpression, leaf_call_offset);
    let recursive_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let main_recur_call = key(unit, generation, &index, NodeKind::CallExpression, 2);

    assert_eq!(
        resolved_item(&db, leaf_path).expect("leaf item"),
        Some(beskid_queries::ResolvedItem { declaration: leaf })
    );
    assert_eq!(
        call_lowering(&db, recursive_call).expect("recursive lowering"),
        Some(beskid_queries::CallLowering::Direct(recur))
    );
    assert_eq!(
        call_lowering(&db, main_recur_call).expect("main recur lowering"),
        Some(beskid_queries::CallLowering::Direct(recur))
    );
    assert_eq!(direct_callees(&db, recur).expect("recursive callees"), Some(Arc::from([recur])));
    assert_eq!(direct_callees(&db, main).expect("main callees"), Some(Arc::from([leaf, recur])));
    let reachable = reachable_items(&db, program, main).expect("reachable query").expect("reachable facts");
    assert_eq!(reachable.as_ref(), &[main, leaf, recur]);
}

#[test]
fn reachable_items_includes_inline_method_callees_without_hir() {
    let source = "type Point { i32 x, i32 Ping() { return 7; } } i32 Main() { return Point { x: 1 }.Ping(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("inline method call"),
        Some(beskid_queries::CallLowering::Direct(method))
    );
    assert_eq!(direct_callees(&db, main).expect("main callees"), Some(Arc::from([method])));
    assert_eq!(direct_callees(&db, method).expect("method callees"), Some(Arc::from([])));
    assert_eq!(
        reachable_items(&db, program, main).expect("reachable query").expect("reachable facts").as_ref(),
        &[main, method]
    );
}

#[test]
fn item_resolution_does_not_cross_local_shadowing_or_unresolved_names() {
    let shadowed_source = r#"i32 Helper() { return 1; }
i32 Main() {
    let Helper = (i32 value) => value;
    return Helper(1);
}"#;
    let (db, _project, unit, generation, index) = setup(shadowed_source);
    let shadowed_offset = shadowed_source.rfind("Helper(1)").expect("shadowed call");
    let shadowed_path = key_at_start(unit, generation, &index, NodeKind::PathExpression, shadowed_offset);
    let shadowed_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(resolved_item(&db, shadowed_path).expect("shadowed item"), None);
    assert_unavailable(call_lowering(&db, shadowed_call));

    let unresolved_source = "i32 Main() { return Missing(); }";
    let (db, _project, unit, generation, index) = setup(unresolved_source);
    let unresolved_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let unresolved_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let unresolved_main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let unresolved_program = key(unit, generation, &index, NodeKind::Program, 0);
    assert_eq!(resolved_item(&db, unresolved_path).expect("unresolved item"), None);
    assert_unavailable(call_lowering(&db, unresolved_call));
    // Unresolved calls are not Direct edges; reachability skips them instead of
    // failing the whole entrypoint walk (see direct_callees_for_item).
    assert_eq!(
        direct_callees(&db, unresolved_main).expect("no direct callees"),
        Some(std::sync::Arc::<[beskid_queries::AstNodeKey]>::from([]))
    );
    assert_eq!(
        reachable_items(&db, unresolved_program, unresolved_main).expect("entrypoint remains reachable"),
        Some(std::sync::Arc::from([unresolved_main]))
    );
}

#[test]
fn item_resolution_prefers_the_nearest_module_and_falls_back_lexically() {
    let source = r#"i32 Helper() { return 0; }
mod Inner {
    i32 Helper() { return 1; }
    i32 Main() { return Helper(); }
    i32 Fallback() { return OuterOnly(); }
}
i32 OuterOnly() { return 2; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let inner_helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let outer_only = key(unit, generation, &index, NodeKind::FunctionDefinition, 4);
    let inner_helper_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("Helper();").expect("inner helper call"),
    );
    let outer_only_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("OuterOnly();").expect("outer fallback call"),
    );

    assert_eq!(
        resolved_item(&db, inner_helper_path).expect("nearest module item"),
        Some(beskid_queries::ResolvedItem { declaration: inner_helper })
    );
    assert_eq!(
        resolved_item(&db, outer_only_path).expect("outer module fallback"),
        Some(beskid_queries::ResolvedItem { declaration: outer_only })
    );
}

#[test]
fn stale_generation_cannot_reuse_item_or_call_graph_facts() {
    let source = "i32 Helper() { return 1; } i32 Main() { return Helper(); }";
    let (mut db, project, unit, generation, index) = setup(source);
    let helper_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert!(resolved_item(&db, helper_path).expect("current item").is_some());
    assert!(direct_callees(&db, main).expect("current callees").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(resolved_item(&db, helper_path).expect("stale item"), None);
    assert_eq!(direct_callees(&db, main).expect("stale callees"), None);
}

#[test]
fn control_flow_facts_follow_ast_branch_termination() {
    let source = r#"
i32 AlwaysReturns(bool condition) {
    if condition { return 1; } else { return 2; }
}
i32 MayFallThrough(bool condition) {
    if condition { return 1; }
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let always_returns = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let may_fall_through = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        control_flow(&db, always_returns).expect("always-returning flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: false })
    );
    assert_eq!(
        control_flow(&db, may_fall_through).expect("fall-through flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: true })
    );
}

#[test]
fn stale_generation_never_observes_semantic_facts() {
    let (mut db, project, unit, generation, index) = setup("i32 Main() { return 0; }");
    let current = key(unit, generation, &index, NodeKind::Literal, 0);
    assert_eq!(node_type(&db, current).expect("current type"), Some(beskid_queries::SemanticTypeId::I32));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 1; }".to_string(),
    )
    .expect("registered syntax edit");
    assert_eq!(node_type(&db, current).expect("stale fact"), None);
}

#[test]
fn local_resolution_never_guesses_from_positions() {
    let source = r#"
i32 First() { let hidden = 1; return hidden; }
i32 Second() { return hidden; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let first_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let second_reference = key(unit, generation, &index, NodeKind::PathExpression, 1);

    assert!(resolved_local(&db, first_reference).expect("first local").is_some());
    assert_eq!(resolved_local(&db, second_reference).expect("out-of-scope local"), None);
}

#[test]
fn local_resolution_uses_generation_safe_declarations_and_lexical_shadowing() {
    let source = r#"i32 Main(i32 value) {
    let first = value;
    if true {
        let value = 2;
        let nested = value;
    }
    return value;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let value_offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    assert_eq!(value_offsets.len(), 5);

    let parameter = key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[0]);
    let inner_declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[2]);
    let parameter_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[1]);
    let inner_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[3]);
    let outer_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[4]);

    assert_eq!(
        resolved_local(&db, parameter_reference).expect("parameter reference"),
        Some(beskid_queries::ResolvedLocal { declaration: parameter })
    );
    assert_eq!(
        resolved_local(&db, inner_reference).expect("shadowed reference"),
        Some(beskid_queries::ResolvedLocal { declaration: inner_declaration })
    );
    assert_eq!(
        resolved_local(&db, outer_reference).expect("outer reference"),
        Some(beskid_queries::ResolvedLocal { declaration: parameter })
    );
}

#[test]
fn local_resolution_covers_lambda_for_and_match_bindings() {
    for source in [
        "i32 Main() { let apply = (i32 value) => value; return apply(1); }",
        "unit Main() { for item in [1] { let copy = item; } }",
        "enum Choice { Some(i32 value), None } i32 Main() { Choice choice = Choice::Some(1); return match choice { Choice::Some(bound) => bound, Choice::None => 0, }; }",
    ] {
        let (db, _project, unit, generation, index) = setup(source);
        let binding_name = if source.contains("value) =>") {
            "value"
        } else if source.contains("for item") {
            "item"
        } else {
            "bound"
        };
        let offsets = source.match_indices(binding_name).map(|(offset, _)| offset).collect::<Vec<_>>();
        assert_eq!(offsets.len(), 2, "{binding_name} occurrences in {source}");
        let declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, offsets[0]);
        let reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, offsets[1]);
        assert_eq!(
            resolved_local(&db, reference).expect("binding reference"),
            Some(beskid_queries::ResolvedLocal { declaration }),
            "binding {binding_name} in {source}"
        );
    }
}

#[test]
fn local_declaration_is_not_visible_in_its_own_initializer() {
    let source = "i32 Main() { let value = value; return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    let initializer_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, offsets[1]);
    assert_eq!(resolved_local(&db, initializer_reference).expect("initializer local"), None);
}

#[test]
fn local_slots_are_stable_within_function_and_distinct_for_lambda_frames() {
    let source = r#"i32 Main(i32 value) {
    let outer = value;
    if true { let outer = 1; }
    let apply = (i32 inner) => inner;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let lambda_owner = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let value_offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    let outer_offsets = source.match_indices("outer").map(|(offset, _)| offset).collect::<Vec<_>>();
    let declarations = [
        key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[0]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offsets[0]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offsets[1]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("apply").expect("apply declaration")),
    ];
    for (slot_index, declaration) in declarations.into_iter().enumerate() {
        assert_eq!(
            local_slot(&db, declaration).expect("function local slot"),
            Some(LocalSlot { owner, index: u32::try_from(slot_index).expect("slot index") })
        );
    }

    let inner =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("inner").expect("lambda parameter"));
    assert_eq!(local_slot(&db, inner).expect("lambda local slot"), Some(LocalSlot { owner: lambda_owner, index: 0 }));
    let function_name =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("Main").expect("function name"));
    assert_eq!(local_slot(&db, function_name).expect("ordinary name"), None);
}

#[test]
fn closure_environment_reports_only_outer_lexical_captures() {
    let source = r#"i32 Main(i32 outer) {
    let copied = outer;
    let apply = (i32 inner) => copied + inner;
    return apply(1);
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let copied_offset = source.find("copied =").expect("copied declaration");
    let copied = key_at_start(unit, generation, &index, NodeKind::Identifier, copied_offset);
    let copied_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("copied +").expect("copied capture use"),
    );
    let inner_offset = source.find("inner) =>").expect("lambda parameter");
    let inner = key_at_start(unit, generation, &index, NodeKind::Identifier, inner_offset);

    let closure = closure_environment(&db, lambda).expect("closure environment").expect("lambda fact");
    assert_eq!(closure.parameters.as_ref(), &[inner]);
    assert_eq!(
        closure.captures.as_ref(),
        &[ClosureCapture {
            declaration: copied,
            slot: local_slot(&db, copied).expect("outer local slot").expect("outer local slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, copied_use).expect("copied use span").expect("copied use span fact"),
        }]
    );
}

#[test]
fn closure_contract_is_generation_bound_and_requires_a_pointer_map_without_claiming_lowering() {
    let source = r#"i32 Main(i32 first, i32 second, string message) {
    let sum = () => first + second;
    let text = () => message;
    return sum();
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let sum = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let text = key(unit, generation, &index, NodeKind::LambdaExpression, 1);
    let first =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("first,").expect("first parameter"));
    let second =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("second,").expect("second parameter"));
    let message = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("message)").expect("message parameter"),
    );
    let sum_body = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let first_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> first +").expect("first capture use") + "=> ".len(),
    );
    let second_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("first + second").expect("second capture use") + "first + ".len(),
    );
    let message_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> message").expect("message reference") + "=> ".len(),
    );

    let sum_contract = closure_signature(&db, sum).expect("sum closure contract").expect("sum closure fact");
    assert_eq!(sum_contract.lambda, sum);
    assert_eq!(sum_contract.lambda.generation, generation);
    assert_eq!(sum_contract.body, sum_body);
    assert_eq!(sum_contract.callable, ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I32 });
    assert_eq!(
        sum_contract.environment.fields.as_ref(),
        &[
            ClosureEnvironmentField {
                capture: ClosureCapture {
                    declaration: first,
                    slot: local_slot(&db, first).expect("first slot").expect("first slot fact"),
                    class: CaptureStorageClass::TransferableValue,
                    span: node_span(&db, first_use).expect("first use span").expect("first use span fact"),
                },
                abi_type: SemanticTypeId::I32,
            },
            ClosureEnvironmentField {
                capture: ClosureCapture {
                    declaration: second,
                    slot: local_slot(&db, second).expect("second slot").expect("second slot fact"),
                    class: CaptureStorageClass::TransferableValue,
                    span: node_span(&db, second_use).expect("second use span").expect("second use span fact"),
                },
                abi_type: SemanticTypeId::I32,
            },
        ]
    );
    assert_eq!(sum_contract.environment.pointer_map, ClosurePointerMapRequirement::RuntimeDescriptorRequired);
    assert_eq!(sum_contract.lowering, ClosureLoweringStatus::NotLowered);
    assert_eq!(sum_contract.allocation, ClosureAllocationStatus::NotAllocated);

    let text_contract = closure_signature(&db, text).expect("text closure contract").expect("text closure fact");
    assert_eq!(text_contract.body, message_reference);
    assert_eq!(
        text_contract.environment.fields.as_ref(),
        &[ClosureEnvironmentField {
            capture: ClosureCapture {
                declaration: message,
                slot: local_slot(&db, message).expect("message slot").expect("message slot fact"),
                class: CaptureStorageClass::TransferableValue,
                span: node_span(&db, message_reference).expect("message use span").expect("message use span fact"),
            },
            abi_type: SemanticTypeId::STRING,
        }]
    );
    assert_eq!(text_contract.environment.pointer_map, ClosurePointerMapRequirement::RuntimeDescriptorRequired);
}

#[test]
fn closure_call_target_and_spawn_entry_validation_use_only_current_syntax_facts() {
    let call_source = "i32 Main() { return ((i32 value) => value)(7); }";
    let (db, _project, unit, generation, index) = setup(call_source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let body = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        call_source.find("=> value").expect("lambda body") + "=> ".len(),
    );
    assert_eq!(
        closure_call_target(&db, call).expect("closure call target"),
        Some(ClosureCallTarget {
            call,
            lambda,
            body,
            callable: ItemSignature { parameters: Arc::from([SemanticTypeId::I32]), result: SemanticTypeId::I32 },
        })
    );

    let spawn_source = "i32 Main() { let task = spawn (() => 7); return 0; }";
    let (db, _project, unit, generation, index) = setup(spawn_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    assert_eq!(
        spawn_entry_validation(&db, spawn).expect("spawn entry validation"),
        Some(SpawnEntryValidation {
            spawn,
            target: lambda,
            callable: Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I32 }),
            is_zero_argument_entry: true,
            diagnostics: Arc::from([]),
        })
    );
}

#[test]
fn spawn_target_preserves_lambda_operand_and_capture_environment() {
    let source = r#"i32 Main(i32 outer) {
    let task = spawn ((i32 inner) => outer + inner);
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let outer_offset = source.find("outer)").expect("parameter declaration");
    let outer = key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offset);
    let outer_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer +").expect("outer capture use") + "=> ".len(),
    );

    let spawn = spawn_target(&db, spawn).expect("spawn target").expect("spawn fact");
    assert_eq!(spawn.callee, lambda);
    assert_eq!(
        spawn.captures.as_ref(),
        &[ClosureCapture {
            declaration: outer,
            slot: local_slot(&db, outer).expect("parameter slot").expect("parameter slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, outer_use).expect("outer use span").expect("outer use span fact"),
        }]
    );
}

#[test]
fn spawn_legality_reports_current_callable_result_and_precise_span() {
    let source = r#"i64 Worker() { return 7_i64; }
i32 Main() { let task = spawn Worker; return 0; }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let worker = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);

    assert_eq!(
        callable_signature(&db, worker).expect("worker signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I64 })
    );

    let legality = spawn_legality(&db, spawn).expect("spawn legality").expect("current spawn fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I64));
    assert_eq!(legality.span, node_span(&db, spawn).expect("spawn span").expect("span"));
    assert!(legality.diagnostics.is_empty());
}

#[test]
fn capture_storage_tracks_nested_shadowed_reference_with_its_exact_span() {
    let source = r#"i32 Main(i32 outer) {
    let make = (i32 outer) => (i32 inner) => outer;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let captured_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer;").map(|offset| offset + "=> ".len()).expect("nested capture reference"),
    );
    let shadowing_parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("outer) =>").expect("shadowing parameter"),
    );

    let capture = capture_storage(&db, captured_reference).expect("capture storage").expect("capture fact");
    assert_eq!(capture.declaration, shadowing_parameter);
    assert_eq!(capture.class, CaptureStorageClass::TransferableValue);
    assert_eq!(capture.span, node_span(&db, captured_reference).expect("reference span").expect("reference span fact"));
}

#[test]
fn closure_environment_reports_nested_shadowed_captures_with_modes_and_spans() {
    let source = r#"i32 Main(i32 outer) {
    let make = (i32 outer) => (i32 inner) => outer;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let outer_lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let inner_lambda = key(unit, generation, &index, NodeKind::LambdaExpression, 1);
    let shadowing_parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("outer) =>").expect("shadowing parameter"),
    );
    let captured_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("=> outer;").map(|offset| offset + "=> ".len()).expect("nested capture reference"),
    );

    let outer_environment =
        closure_environment(&db, outer_lambda).expect("outer closure environment").expect("outer lambda fact");
    assert!(
        outer_environment.captures.is_empty(),
        "outer lambda binds shadowing outer and must not capture Main's parameter"
    );

    let inner_environment =
        closure_environment(&db, inner_lambda).expect("inner closure environment").expect("inner lambda fact");
    assert_eq!(
        inner_environment.captures.as_ref(),
        &[ClosureCapture {
            declaration: shadowing_parameter,
            slot: local_slot(&db, shadowing_parameter).expect("shadowing slot").expect("shadowing slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, captured_reference).expect("reference span").expect("reference span fact"),
        }]
    );
}

#[test]
fn spawn_legality_rejects_non_callable_and_stack_capture_with_precise_diagnostics() {
    let non_callable_source = "i32 Main() { let task = spawn 7; return 0; }";
    let (db, _project, unit, generation, index) = setup(non_callable_source);
    let non_callable_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let non_callable =
        spawn_legality(&db, non_callable_spawn).expect("non-callable spawn legality").expect("non-callable spawn fact");
    assert!(!non_callable.is_legal());
    assert_eq!(non_callable.result, None);
    assert_eq!(non_callable.diagnostics.len(), 1);
    assert_eq!(non_callable.diagnostics[0].kind, SpawnDiagnosticKind::TargetNotCallable);
    assert_eq!(
        non_callable.diagnostics[0].span,
        node_span(&db, non_callable_spawn).expect("spawn span").expect("spawn span fact")
    );

    let parameterized_source = "i32 Main() { let task = spawn ((i32 value) => value); return 0; }";
    let (db, _project, unit, generation, index) = setup(parameterized_source);
    let parameterized_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let parameterized = spawn_legality(&db, parameterized_spawn)
        .expect("parameterized spawn legality")
        .expect("parameterized spawn fact");
    assert!(!parameterized.is_legal());
    assert_eq!(parameterized.result, Some(SemanticTypeId::I32));
    assert_eq!(parameterized.diagnostics.len(), 1);
    assert_eq!(parameterized.diagnostics[0].kind, SpawnDiagnosticKind::TargetRequiresArguments);
    assert_eq!(
        parameterized.diagnostics[0].span,
        node_span(&db, parameterized_spawn).expect("parameterized spawn span").expect("parameterized spawn span fact")
    );

    let capture_source = r#"i32 Main(pointer frame) {
    let task = spawn (() => frame);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(capture_source);
    let capture_spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let capture_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        capture_source.rfind("frame)").expect("captured pointer reference"),
    );
    let capture =
        capture_storage(&db, capture_reference).expect("pointer capture storage").expect("pointer capture fact");
    assert_eq!(capture.class, CaptureStorageClass::StackReference);

    let illegal_capture =
        spawn_legality(&db, capture_spawn).expect("capturing spawn legality").expect("capturing spawn fact");
    assert!(!illegal_capture.is_legal());
    assert_eq!(illegal_capture.result, Some(SemanticTypeId::POINTER));
    assert_eq!(illegal_capture.diagnostics.len(), 1);
    assert_eq!(illegal_capture.diagnostics[0].kind, SpawnDiagnosticKind::StackReferenceEscapesSpawn);
    assert_eq!(illegal_capture.diagnostics[0].span, capture.span);
}

#[test]
fn stale_generation_never_reuses_spawn_legality_or_capture_storage() {
    let source = r#"i32 Main(i32 value) {
    let task = spawn (() => value);
    return 0;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let capture_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("value)").expect("capture reference"),
    );
    assert!(spawn_legality(&db, spawn).expect("current spawn").is_some());
    assert!(capture_storage(&db, capture_reference).expect("current capture").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("registered syntax edit");

    assert_eq!(spawn_legality(&db, spawn).expect("stale spawn"), None);
    assert_eq!(capture_storage(&db, capture_reference).expect("stale capture"), None);
}

#[test]
fn spawn_legality_normalizes_empty_call_entries_and_rejects_call_arguments() {
    let empty_call_source = r#"i64 Worker() { return 7_i64; }
i32 Main() { let task = spawn Worker(); return 0; }"#;
    let (db, _project, unit, generation, index) = setup(empty_call_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let worker_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        empty_call_source.find("spawn Worker()").map(|offset| offset + "spawn ".len()).expect("empty-call Worker path"),
    );

    let target = spawn_target(&db, spawn).expect("empty-call spawn target").expect("empty-call spawn fact");
    assert_ne!(target.callee, call, "empty-arg spawn call must not keep the CallExpression as the fiber entry");
    assert_eq!(target.callee, worker_path, "empty-arg spawn call must unwrap to the entry path operand");
    assert_eq!(node_kind(&db, target.callee).expect("entry kind"), Some(NodeKind::PathExpression));
    assert!(target.captures.is_empty());

    let legality = spawn_legality(&db, spawn).expect("empty-call spawn legality").expect("empty-call legality fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I64));
    assert_eq!(
        legality.span,
        node_span(&db, spawn).expect("empty-call spawn span").expect("empty-call spawn span fact")
    );

    let entry = spawn_entry_validation(&db, spawn).expect("empty-call spawn entry").expect("empty-call entry fact");
    assert_eq!(entry.target, worker_path);
    assert_eq!(entry.callable, Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::I64 }));
    assert!(entry.is_zero_argument_entry);

    let args_source = r#"i64 Worker(i64 value) { return value; }
i32 Main() { let task = spawn Worker(7_i64); return 0; }"#;
    let (db, _project, unit, generation, index) = setup(args_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    let target = spawn_target(&db, spawn).expect("argful spawn target").expect("argful spawn fact");
    assert_eq!(target.callee, call, "spawn call arguments stay on the CallExpression so legality can fail closed");

    let legality = spawn_legality(&db, spawn).expect("argful spawn legality").expect("argful legality fact");
    assert!(!legality.is_legal());
    assert_eq!(legality.result, None);
    assert_eq!(legality.diagnostics.len(), 1);
    assert_eq!(legality.diagnostics[0].kind, SpawnDiagnosticKind::CalleeArgumentsUnsupported);
    assert_eq!(
        legality.diagnostics[0].span,
        node_span(&db, spawn).expect("argful spawn span").expect("argful spawn span fact")
    );
}

#[test]
fn spawn_legality_accepts_transferable_captures_and_rejects_mutable_stack_escapes() {
    let legal_source = r#"i32 Main(i32 value) {
    let task = spawn (() => value);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(legal_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let value = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        legal_source.find("value)").expect("parameter declaration"),
    );
    let value_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        legal_source.find("=> value)").map(|offset| offset + "=> ".len()).expect("transferable capture use"),
    );

    let legality =
        spawn_legality(&db, spawn).expect("transferable spawn legality").expect("transferable legality fact");
    assert!(legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I32));
    assert_eq!(
        legality.target.captures.as_ref(),
        &[ClosureCapture {
            declaration: value,
            slot: local_slot(&db, value).expect("value slot").expect("value slot fact"),
            class: CaptureStorageClass::TransferableValue,
            span: node_span(&db, value_use).expect("value use span").expect("value use span fact"),
        }]
    );

    let mutable_source = r#"i32 Main() {
    mut i32 frame = 1;
    let task = spawn (() => frame);
    return 0;
}"#;
    let (db, _project, unit, generation, index) = setup(mutable_source);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    let frame_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        mutable_source.rfind("frame)").expect("mutable capture reference"),
    );
    let capture = capture_storage(&db, frame_use).expect("mutable capture storage").expect("mutable capture fact");
    assert_eq!(capture.class, CaptureStorageClass::StackReference);

    let legality = spawn_legality(&db, spawn).expect("mutable spawn legality").expect("mutable legality fact");
    assert!(!legality.is_legal());
    assert_eq!(legality.result, Some(SemanticTypeId::I32));
    assert_eq!(legality.diagnostics.len(), 1);
    assert_eq!(legality.diagnostics[0].kind, SpawnDiagnosticKind::StackReferenceEscapesSpawn);
    assert_eq!(legality.diagnostics[0].span, capture.span);
    assert_eq!(legality.diagnostics[0].capture, Some(capture));
}

#[test]
fn stale_generation_never_reuses_closure_contract_or_spawn_entry_validation() {
    let source = r#"i32 Main(i32 value) {
    let closure = () => value;
    let task = spawn (() => value);
    return closure();
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let closure = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let spawn = key(unit, generation, &index, NodeKind::SpawnExpression, 0);
    assert!(closure_signature(&db, closure).expect("current closure").is_some());
    assert!(spawn_entry_validation(&db, spawn).expect("current spawn entry").is_some());

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("registered syntax edit");

    assert_eq!(closure_signature(&db, closure).expect("stale closure"), None);
    assert_eq!(spawn_entry_validation(&db, spawn).expect("stale spawn entry"), None);
}

#[test]
fn runtime_intrinsic_uses_the_manifest_owned_builtin_index() {
    let source = "i32 Main() { __str_len(\"value\"); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let expected =
        beskid_analysis::builtins::builtin_for_path(&["__str_len".to_string()]).expect("generated builtin").0;

    assert_eq!(
        runtime_intrinsic(&db, call).expect("runtime intrinsic"),
        Some(beskid_queries::RuntimeIntrinsic(expected as u32))
    );
    assert_eq!(
        call_lowering(&db, call).expect("manifest builtin call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
}

#[test]
fn corelib_syscall_source_gets_a_distinct_service_lowering_but_app_code_cannot_forge_it() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("corelib project").keep();
    let source = canonical_corelib_syscall_sources().pop().expect("embedded Core.Syscall source");
    let source_path = directory.join("Syscall.bd");
    std::fs::write(&source_path, &source.source).expect("write Core.Syscall source");
    let program = parse_program(&source.source).expect("parse Core.Syscall source");
    let generation = SyntaxGenerationId(71);
    let index = SyntaxIndex::from_program(&program, generation);
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "corelib-source".into(),
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source.clone(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    build_canonical_corelib_syscall_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_syscall_service_capability(&manifest).expect("Corelib authority"),
    )
    .expect("exact Core.Syscall source obtains service authority");

    let syscall_write = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: SourceUnitId::new(&db, source_path.clone()), generation, node })
        .find(|key| {
            matches!(
                call_lowering(&db, *key).expect("Core.Syscall lowering"),
                Some(beskid_queries::CallLowering::CorelibService(service))
                    if service.name == "__syscall_write"
            )
        })
        .expect("Core.Syscall write call");
    assert!(matches!(
        call_lowering(&db, syscall_write).expect("Core.Syscall lowering"),
        Some(beskid_queries::CallLowering::CorelibService(_))
    ));

    let (ordinary_db, _project, ordinary_unit, ordinary_generation, ordinary_index) =
        setup("i64 Main() { return __syscall_write(1, \"not corelib\"); }");
    let ordinary_call = key(ordinary_unit, ordinary_generation, &ordinary_index, NodeKind::CallExpression, 0);
    assert_eq!(
        call_lowering(&ordinary_db, ordinary_call).expect("ordinary syscall lowering"),
        Some(beskid_queries::CallLowering::Dynamic),
        "an application spelling must not gain the Corelib service capability"
    );

    let mut forged_db = BeskidDatabase::default();
    let forged_directory = tempfile::tempdir().expect("forged Corelib project").keep();
    let forged_path = forged_directory.join("Syscall.bd");
    let forged_source = source.source.replacen("__syscall_write", "__syscall_writex", 1);
    std::fs::write(&forged_path, &forged_source).expect("write forged Corelib source");
    let forged_program = parse_program(&forged_source).expect("parse forged Corelib source");
    let forged_project = ProjectSession::new(
        &forged_db,
        forged_directory.clone(),
        forged_path.clone(),
        "beskid-corelib".into(),
        "forged-corelib-source".into(),
    );
    let forged_assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: forged_directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            path: forged_path,
            source: forged_source,
            program: forged_program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    assert!(
        build_canonical_corelib_syscall_typed_program(
            &mut forged_db,
            forged_project,
            SyntaxGenerationId(72),
            forged_assembly,
            canonical_corelib_syscall_service_capability(&manifest).expect("Corelib authority for forge check"),
        )
        .is_err(),
        "altering the Corelib source must not mint its service capability"
    );
}

#[test]
fn corelib_service_authority_is_registered_for_only_the_exact_syscall_unit_in_an_assembly() {
    let source = canonical_corelib_syscall_sources().pop().expect("embedded Core.Syscall source");
    let workspace = tempfile::tempdir().expect("Corelib assembly workspace").keep();
    let application_root = workspace.join("application");
    let syscall_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_SYSCALL_SOURCE_PATH)
        .expect("compiler-owned Core.Syscall path");
    let foundation_root = syscall_path.ancestors().nth(3).expect("foundation source root").to_path_buf();
    let application_path = application_root.join("Main.bd");
    let application_source = "i64 Main() { return __syscall_write(1, \"application\"); }";
    let syscall_program = parse_program(&source.source).expect("parse embedded Core.Syscall");
    let application_program = parse_program(application_source).expect("parse application source");
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: application_root.clone() },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: foundation_root.clone(),
            }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Core/Syscall/Syscall.bd".into(),
                path: syscall_path.clone(),
                source: source.source.clone(),
                program: syscall_program.clone(),
            },
            SourceUnit {
                logical_name: "Main.bd".into(),
                path: application_path.clone(),
                source: application_source.into(),
                program: application_program.clone(),
            },
        ]),
        1,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let generation = SyntaxGenerationId(73);
    let mut db = BeskidDatabase::default();
    let project = ProjectSession::new(
        &db,
        application_root.clone(),
        application_path.clone(),
        "corelib-assembly".into(),
        "exact-syscall-unit".into(),
    );
    let typed = build_typed_program_with_corelib_syscall_services(
        &mut db,
        project,
        generation,
        Arc::clone(&assembly),
        canonical_corelib_syscall_service_capability(&manifest).expect("Corelib authority"),
    )
    .expect("multi-unit assembly obtains Corelib service authority");
    assert!(typed.runtime_intrinsic_capability.is_none());
    assert!(typed.corelib_service_capability.is_some());

    let syscall_index = SyntaxIndex::from_program(&syscall_program, generation);
    let syscall_call = syscall_index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: SourceUnitId::new(&db, syscall_path.clone()), generation, node })
        .find(|key| {
            matches!(
                call_lowering(&db, *key).expect("Core.Syscall lowering"),
                Some(beskid_queries::CallLowering::CorelibService(service))
                    if service.name == "__syscall_write"
            )
        })
        .expect("exact syscall write call");
    assert!(matches!(
        call_lowering(&db, syscall_call).expect("Core.Syscall service lowering"),
        Some(beskid_queries::CallLowering::CorelibService(_))
    ));

    let application_index = SyntaxIndex::from_program(&application_program, generation);
    let application_call =
        application_index.ids_of_kind(NodeKind::CallExpression).next().expect("application syscall spelling");
    assert_eq!(
        call_lowering(
            &db,
            AstNodeKey { unit: SourceUnitId::new(&db, application_path.clone()), generation, node: application_call },
        )
        .expect("application lowering"),
        Some(beskid_queries::CallLowering::Dynamic),
        "only the embedded Core.Syscall unit receives service authority"
    );

    let mut forged_db = BeskidDatabase::default();
    let forged_source = application_source.to_owned();
    let forged_program = parse_program(&forged_source).expect("parse forged syscall source");
    let forged_assembly = Arc::new(SyntaxProgramAssembly::new(
        assembly.roots().clone(),
        Arc::new(vec![SourceUnit {
            logical_name: "Core/Syscall/Syscall.bd".into(),
            path: syscall_path.clone(),
            source: forged_source,
            program: forged_program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let forged_project = ProjectSession::new(
        &forged_db,
        application_root,
        syscall_path.clone(),
        "corelib-assembly".into(),
        "forged-syscall-unit".into(),
    );
    let forged_typed = build_typed_program_with_corelib_syscall_services(
        &mut forged_db,
        forged_project,
        SyntaxGenerationId(74),
        forged_assembly,
        canonical_corelib_syscall_service_capability(&manifest).expect("forge authority"),
    )
    .expect("forged unit stays an ordinary syntax program");
    assert!(forged_typed.corelib_service_capability.is_none());
    let forged_index = SyntaxIndex::from_program(&forged_program, SyntaxGenerationId(74));
    let forged_call = forged_index.ids_of_kind(NodeKind::CallExpression).next().expect("forged syscall call");
    assert_eq!(
        call_lowering(
            &forged_db,
            AstNodeKey {
                unit: SourceUnitId::new(&forged_db, syscall_path),
                generation: SyntaxGenerationId(74),
                node: forged_call,
            },
        )
        .expect("forged lowering"),
        Some(beskid_queries::CallLowering::Dynamic),
        "altered Core.Syscall bytes cannot receive service authority"
    );
}

#[test]
fn syntax_only_signatures_preserve_runtime_pointer_and_never_primitives() {
    let source = "pointer Echo(pointer value) { return value; } never Stop() { while true {} }";
    let (db, _project, unit, generation, index) = setup(source);
    let echo = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let stop = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        item_signature(&db, echo).expect("pointer signature"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        item_signature(&db, stop).expect("never signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::NEVER })
    );
}

#[test]
fn local_slots_cover_methods_for_iterators_and_match_bindings() {
    let method_source = r#"type Value { i32 raw }
impl Value { i32 Sum(i32 first) { let local = first; return local; } }"#;
    let (db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    for (name, expected_index) in [("first", 0), ("local", 1)] {
        let declaration = key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            method_source.find(name).expect("method declaration"),
        );
        assert_eq!(
            local_slot(&db, declaration).expect("method local slot"),
            Some(LocalSlot { owner: method, index: expected_index })
        );
    }

    for (source, declarations) in [
        ("unit Main() { for item in [1] { let copy = item; } }", [("item", 0), ("copy", 1)]),
        (
            "enum Choice { Some(i32 value), None } i32 Main() { Choice choice = Choice::Some(1); return match choice { Choice::Some(bound) => bound, Choice::None => 0, }; }",
            [("choice", 0), ("bound", 1)],
        ),
    ] {
        let (db, _project, unit, generation, index) = setup(source);
        let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
        for (name, expected_index) in declarations {
            let declaration = key_at_start(
                unit,
                generation,
                &index,
                NodeKind::Identifier,
                source.find(name).expect("binding declaration"),
            );
            assert_eq!(
                local_slot(&db, declaration).expect("binding local slot"),
                Some(LocalSlot { owner, index: expected_index }),
                "binding {name} in {source}"
            );
        }
    }
}

#[test]
fn for_iterator_fact_proves_range_element_type_and_shadowing() {
    use beskid_queries::ForIteratorFact;

    let source = "i32 Main() { let value = 1_i64; for value in range(1, 4) { let copy = value; } return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("for value").expect("for header") + "for ".len(),
    );
    assert_eq!(
        for_iterator_fact(&db, for_stmt).expect("for iterator fact"),
        Some(ForIteratorFact { declaration, element_type: SemanticTypeId::I32 })
    );
    assert_eq!(node_type(&db, declaration).expect("iterator declaration type"), Some(SemanticTypeId::I32));
    let body_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("= value").expect("body use") + "= ".len(),
    );
    assert_eq!(
        resolved_local(&db, body_reference).expect("iterator reference").map(|resolved| resolved.declaration),
        Some(declaration)
    );
    assert_eq!(node_type(&db, body_reference).expect("shadowed iterator type"), Some(SemanticTypeId::I32));

    let nested = "i32 Main() { for outer in range(1, 3) { for outer in range(10_i64, 12_i64) { let inner = outer; } } return 0; }";
    let (db, _project, unit, generation, index) = setup(nested);
    let outer_for = key(unit, generation, &index, NodeKind::ForStatement, 0);
    let inner_for = key(unit, generation, &index, NodeKind::ForStatement, 1);
    assert_eq!(
        for_iterator_fact(&db, outer_for).expect("outer for").map(|fact| fact.element_type),
        Some(SemanticTypeId::I32)
    );
    assert_eq!(
        for_iterator_fact(&db, inner_for).expect("inner for").map(|fact| fact.element_type),
        Some(SemanticTypeId::I64)
    );
    let inner_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        nested.find("= outer").expect("inner use") + "= ".len(),
    );
    assert_eq!(node_type(&db, inner_use).expect("nested shadow type"), Some(SemanticTypeId::I64));
}

#[test]
fn stale_generation_cannot_reuse_for_iterator_fact() {
    let source = "i32 Main() { for value in range(1, 4) { let copy = value; } return 0; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    assert!(for_iterator_fact(&db, for_stmt).expect("current for iterator").is_some());
    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { for other in range(1, 4) { let copy = other; } return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(for_iterator_fact(&db, for_stmt).expect("stale for iterator"), None);
}

#[test]
fn for_iterator_fact_rejects_non_range_iterables() {
    let source = "unit Main() { for item in [1] { let copy = item; } }";
    let (db, _project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    assert_unavailable(for_iterator_fact(&db, for_stmt));
}

#[test]
fn stale_generation_cannot_reuse_a_local_slot_identity() {
    let source = "i32 Main() { let value = 1; return value; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let declaration =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("value").expect("local declaration"));
    let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert!(resolved_local(&db, reference).expect("current local").is_some());
    assert_eq!(local_slot(&db, declaration).expect("current local slot"), Some(LocalSlot { owner, index: 0 }));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { let other = 1; return other; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(resolved_local(&db, reference).expect("stale local"), None);
    assert_eq!(local_slot(&db, declaration).expect("stale slot"), None);
}

#[test]
fn mutable_local_assignment_requires_a_current_mutable_lexical_declaration() {
    let source = "i32 Main() { mut i32 total = 0; total = total + 1; return total; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("total").expect("mutable declaration"),
    );
    assert_eq!(
        mutable_local_assignment(&db, assignment).expect("mutable assignment fact"),
        Some(MutableLocalAssignment {
            declaration,
            slot: local_slot(&db, declaration)
                .expect("mutable declaration slot")
                .expect("mutable declaration slot fact"),
        })
    );

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { i32 total = 0; total = total + 1; return total; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(
        mutable_local_assignment(&db, assignment).expect("stale assignment fact"),
        None,
        "a stale assignment key cannot retain write authority"
    );
}

#[test]
fn immutable_local_assignment_is_an_explicit_unavailable_syntax_fact() {
    let source = "i32 Main() { i32 total = 0; total = total + 1; return total; }";
    let (db, _project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    assert_unavailable(mutable_local_assignment(&db, assignment));
}

#[test]
fn operator_facts_cover_expression_selection() {
    let source = "bool Main() { let value = 1 + 2; return !(value == 3); }";
    let (db, _project, unit, generation, index) = setup(source);
    let add = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let equals = key(unit, generation, &index, NodeKind::BinaryExpression, 1);
    let not = key(unit, generation, &index, NodeKind::UnaryExpression, 0);

    assert_eq!(operator_fact(&db, add).expect("operator"), Some(OperatorFact::Add));
    assert_eq!(operator_fact(&db, equals).expect("operator"), Some(OperatorFact::Eq));
    assert_eq!(operator_fact(&db, not).expect("operator"), Some(OperatorFact::Not));
}

#[test]
fn string_interpolation_desugar_uses_string_add_facts() {
    let source = r#"
string Prefix() { return "x"; }
string Main(string body) { return "${Prefix()}${body}!"; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let outer = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let inner = key(unit, generation, &index, NodeKind::BinaryExpression, 1);

    assert_eq!(operator_fact(&db, inner).expect("inner string add"), Some(OperatorFact::StringAdd));
    assert_eq!(operator_fact(&db, outer).expect("outer string add"), Some(OperatorFact::StringAdd));
    assert_eq!(abi_type(&db, inner).expect("inner abi"), Some(SemanticTypeId::STRING));
    assert_eq!(abi_type(&db, outer).expect("outer abi"), Some(SemanticTypeId::STRING));
    assert_eq!(node_type(&db, outer).expect("outer node type"), Some(SemanticTypeId::STRING));
}

#[test]
fn item_body_is_the_exact_function_and_method_body_child() {
    let function_source = "i32 Main() { return 0; }";
    let (function_db, _project, unit, generation, index) = setup(function_source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let function_program =
        expand_program(parse_program(function_source).expect("function parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let function_snapshot = SyntaxSnapshot::from_program(&function_program, generation.0);
    let function_node = function_snapshot
        .node_at(function.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .expect("function definition");
    let expected_function_body =
        function_snapshot.id_of(DynNodeRef::from(&function_node.body)).expect("exact function body");
    assert_eq!(
        item_body(&function_db, function).expect("function body"),
        Some(AstNodeKey { node: beskid_analysis::syntax::AstNodeId(expected_function_body), ..function })
    );

    let method_source = "type Value { i32 raw } impl Value { i32 Get() { return this.raw; } }";
    let (method_db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let method_program =
        expand_program(parse_program(method_source).expect("method parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let method_snapshot = SyntaxSnapshot::from_program(&method_program, generation.0);
    let method_node = method_snapshot
        .node_at(method.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
        .expect("method definition");
    let expected_method_body = method_snapshot.id_of(DynNodeRef::from(&method_node.body)).expect("exact method body");
    assert_eq!(
        item_body(&method_db, method).expect("method body"),
        Some(AstNodeKey { node: beskid_analysis::syntax::AstNodeId(expected_method_body), ..method })
    );
}
