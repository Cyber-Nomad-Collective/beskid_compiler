use super::support::{assert_unavailable, key, key_at_start, setup};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, ProgramAssembly,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AggregateFieldShape, AstNodeKey, BeskidDatabase, EnumLayoutFact, EnumMatchArmFact, EnumMatchFact,
    EnumVariantLayoutFact, ItemSignature, ProjectSession, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type,
    aggregate_field_access, aggregate_layout, build_typed_program, call_arguments, enum_constructor, enum_layout,
    enum_match, generic_call_specialization, item_abi_signature,
};
use std::path::PathBuf;
use std::sync::Arc;

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
            signature: ItemSignature {
                parameters: Arc::from([SemanticTypeId::I64, SemanticTypeId::I64]),
                result: SemanticTypeId::UNIT,
            },
            substitutions: Arc::from([beskid_queries::GenericSubstitution {
                parameter: Arc::from("T"),
                argument: SemanticTypeId::I64,
            }]),
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
    let source = include_str!("../../../beskid_tests/fixtures/mods/sample_mod/Src/Mod.bd");
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
        Some(beskid_queries::EnumConstructorFact { declaration: result, variant_index: 0, payload: Some(payload) }),
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
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
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
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
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
        Some(beskid_queries::EnumConstructorFact { declaration, variant_index: 1, payload: Some(payload) })
    );
}

#[test]
fn enum_constructor_rejects_multiple_payloads_until_isle_has_a_multi_field_shape() {
    let source = "enum Pair { Value(i32 left, i32 right) } i32 Main() { Pair pair = Pair::Value(1, 2); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let constructor = key(unit, generation, &index, NodeKind::EnumConstructorExpression, 0);

    assert_unavailable(enum_constructor(&db, constructor));
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
                EnumMatchArmFact { variant_index: Some(0), body: first_body, binding: None },
                EnumMatchArmFact { variant_index: Some(1), body: second_body, binding: None },
            ]),
        })
    );
}
