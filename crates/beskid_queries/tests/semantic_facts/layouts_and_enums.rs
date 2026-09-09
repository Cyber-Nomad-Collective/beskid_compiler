use super::support::{assert_unavailable, key, key_at_start, setup};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AggregateFieldShape, AstNodeKey, BeskidDatabase, EnumLayoutFact, EnumMatchArmFact, EnumMatchFact,
    EnumMatchPatternFact, EnumMatchVariantPatternFact, EnumVariantLayoutFact, ItemSignature, LiteralFact,
    ProjectSession, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type, aggregate_field_access,
    aggregate_layout, aggregate_literal_layout, array_index_element_specialization, build_typed_program,
    call_arguments, contextual_integer_literal_abi_type, enum_constructor, enum_layout, enum_match,
    generic_call_specialization, implicit_method_receiver, item_abi_signature,
};
use std::path::PathBuf;
use std::sync::Arc;

fn enum_pattern_variant_index(arm: &EnumMatchArmFact) -> u32 {
    let EnumMatchPatternFact::Enum(pattern) = &arm.pattern else {
        panic!("expected enum arm pattern");
    };
    pattern.variant_index
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
fn phantom_generic_aggregate_literal_materializes_its_source_independent_layout() {
    let source = "type ArrayIter<T> { i64 index, i64 length } ArrayIter<T> Make<T>() { return ArrayIter<T> { index: 0, length: 1 }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::StructLiteralExpression, 0);
    let zero = key(unit, generation, &index, NodeKind::LiteralExpression, 0);

    assert_eq!(
        aggregate_literal_layout(&db, literal).expect("phantom generic literal layout query"),
        Some(beskid_queries::AggregateLayoutFact {
            fields: Arc::from([
                (Arc::from("index"), AggregateFieldShape::Scalar(SemanticTypeId::I64)),
                (Arc::from("length"), AggregateFieldShape::Scalar(SemanticTypeId::I64)),
            ]),
        }),
    );
    assert_eq!(
        contextual_integer_literal_abi_type(&db, zero).expect("phantom generic field context"),
        Some(SemanticTypeId::I64),
    );
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
            substitutions: Arc::from([beskid_queries::GenericSubstitution::inferred("T", SemanticTypeId::I64,)]),
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
fn method_owned_fields_resolve_through_the_implicit_receiver() {
    let source = r#"
type List<T> {
    T[] storage,
    i64 count,

    unit Probe(i64 index) {
        mut T[] nextStorage = storage;
        if index < 0 || index >= count {
            return;
        }
        return;
    }
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let declaration = key(unit, generation, &index, NodeKind::TypeDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let storage =
        key_at_start(unit, generation, &index, NodeKind::PathExpression, source.find("storage;").expect("storage use"));
    let count =
        key_at_start(unit, generation, &index, NodeKind::PathExpression, source.rfind("count").expect("count use"));

    for (field, expected_index, expected_abi) in
        [(storage, 0, SemanticTypeId::POINTER), (count, 1, SemanticTypeId::I64)]
    {
        let access = aggregate_field_access(&db, field)
            .expect("implicit field access query")
            .expect("method-owned field access");
        assert_eq!(access.declaration, declaration);
        assert_eq!(access.receiver, method, "the MethodDefinition owns the implicit receiver");
        assert_eq!(access.index, expected_index);
        assert_eq!(abi_type(&db, field), Ok(Some(expected_abi)));
    }
}

#[test]
fn self_path_resolves_to_its_implicit_method_receiver() {
    let source = "type List<T> { List<T> Identity() { return self; } }";
    let (db, _project, unit, generation, index) = setup(source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let receiver = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(implicit_method_receiver(&db, receiver), Ok(Some(method)));
}

#[test]
fn generic_array_index_element_resolves_through_the_enclosing_specialization() {
    let source = "T Get<T>(T[] values, i64 index) { return values[index]; }";
    let (db, _project, unit, generation, index) = setup(source);
    let indexed = key(unit, generation, &index, NodeKind::IndexExpression, 0);

    assert_eq!(
        array_index_element_specialization(
            &db,
            indexed,
            Arc::from([beskid_queries::GenericSubstitution::inferred("T", SemanticTypeId::I64)]),
        )
        .expect("contextual array-index element query"),
        Some(SemanticTypeId::I64),
    );
}

#[test]
fn sample_mod_method_abi_signatures_include_pointer_receiver_and_nominal_parameter() {
    let source = include_str!("../../../beskid_tests_mods/fixtures/mods/sample_mod/Src/Mod.bd");
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
    assert_eq!(enum_pattern_variant_index(&fact.arms[0]), 0);
    assert_eq!(enum_pattern_variant_index(&fact.arms[1]), 1);
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
    let generation = SyntaxGenerationId(98);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
    assert_eq!(enum_pattern_variant_index(&fact.arms[0]), 0);
    assert_eq!(enum_pattern_variant_index(&fact.arms[1]), 1);
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
    let generation = SyntaxGenerationId(137);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
fn fully_qualified_enum_resolution_rejects_an_ambiguous_assembled_module_path() {
    let mut db = BeskidDatabase::default();
    let fixture = PathBuf::from("/tmp/ambiguous-qualified-enum-resolution");
    let host_root = fixture.join("host/src");
    let first_dependency_root = fixture.join("dependency-a/src");
    let second_dependency_root = fixture.join("dependency-b/src");
    let main_path = host_root.join("Main.bd");
    let first_results_path = first_dependency_root.join("Core/Results/Results.bd");
    let second_results_path = second_dependency_root.join("Core/Results/Results.bd");
    let main_source = r#"
unit Main() {
    Core.Results.Result<i64, string> result = Core.Results.Result<i64, string>::Ok(1_i64);
    return;
}
"#;
    let results_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    let sources =
        [(&main_path, main_source), (&first_results_path, results_source), (&second_results_path, results_source)];
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
    let generation = SyntaxGenerationId(138);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: host_root.clone() },
            dependencies: vec![
                RootEntry { dependency_name: Some("results-a".into()), source_root: first_dependency_root },
                RootEntry { dependency_name: Some("results-b".into()), source_root: second_dependency_root },
            ],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let main_unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        fixture.join("host"),
        main_unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let main_index = SyntaxIndex::from_program(&main_program, generation);
    let constructor = key(main_unit, generation, &main_index, NodeKind::EnumConstructorExpression, 0);

    assert_unavailable(enum_layout(&db, constructor));
    assert_unavailable(enum_constructor(&db, constructor));
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
    let first = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let second = key(unit, generation, &index, NodeKind::LiteralExpression, 1);

    assert_eq!(
        enum_constructor(&db, constructor).expect("multi-field enum constructor query"),
        Some(beskid_queries::EnumConstructorFact {
            declaration,
            variant_index: 0,
            payloads: Arc::from([first, second]),
        })
    );
}

#[test]
fn enum_constructor_contextualizes_an_unsuffixed_integer_at_its_exact_payload_position() {
    let source = "enum EnvironmentError { UnsupportedMutation(string name, i64 hostReason) } unit Main(string name) { EnvironmentError error = EnvironmentError::UnsupportedMutation(name, 0); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let reason = key(unit, generation, &index, NodeKind::LiteralExpression, 0);

    assert_eq!(
        contextual_integer_literal_abi_type(&db, reason).expect("second enum payload context"),
        Some(SemanticTypeId::I64),
    );
}

#[test]
fn enum_constructor_contextualizes_grouped_and_nested_integer_payloads_at_their_own_boundaries() {
    let source = r#"
enum Inner { Code(i32 code) }
enum Outer { Pair(i64 wide, i32 narrow), Wrap(Inner inner) }
unit Main() {
    Outer pair = Outer::Pair((1), (2));
    Outer wrapped = Outer::Wrap(Inner::Code((3)));
    return;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let wide = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let narrow = key(unit, generation, &index, NodeKind::LiteralExpression, 1);
    let nested = key(unit, generation, &index, NodeKind::LiteralExpression, 2);

    assert_eq!(
        contextual_integer_literal_abi_type(&db, wide).expect("grouped first payload"),
        Some(SemanticTypeId::I64)
    );
    assert_eq!(
        contextual_integer_literal_abi_type(&db, narrow).expect("grouped second payload"),
        Some(SemanticTypeId::I32),
    );
    assert_eq!(
        contextual_integer_literal_abi_type(&db, nested).expect("nested enum payload"),
        Some(SemanticTypeId::I32),
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
                EnumMatchArmFact {
                    pattern: EnumMatchPatternFact::Enum(EnumMatchVariantPatternFact {
                        declaration,
                        layout: EnumLayoutFact {
                            variants: Arc::from([
                                EnumVariantLayoutFact { name: Arc::from("None"), fields: Arc::from([]) },
                                EnumVariantLayoutFact { name: Arc::from("Some"), fields: Arc::from([]) },
                            ]),
                        },
                        variant_index: 0,
                        items: Arc::from([]),
                    }),
                    body: first_body,
                },
                EnumMatchArmFact {
                    pattern: EnumMatchPatternFact::Enum(EnumMatchVariantPatternFact {
                        declaration,
                        layout: EnumLayoutFact {
                            variants: Arc::from([
                                EnumVariantLayoutFact { name: Arc::from("None"), fields: Arc::from([]) },
                                EnumVariantLayoutFact { name: Arc::from("Some"), fields: Arc::from([]) },
                            ]),
                        },
                        variant_index: 1,
                        items: Arc::from([]),
                    }),
                    body: second_body,
                },
            ]),
        })
    );
}

#[test]
fn enum_match_materializes_unit_and_typed_scalar_literal_patterns() {
    let source = "enum Result { Ok(unit value), Error(i64 error) } bool Main(Result result) { return match result { Result::Ok(()) => true, Result::Error(7_i64) => false, _ => false, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let fact = enum_match(&db, expression).expect("enum match query").expect("recursive literal patterns");

    let EnumMatchPatternFact::Enum(ok) = &fact.arms[0].pattern else {
        panic!("Ok arm must be an enum pattern");
    };
    assert!(matches!(ok.items.as_ref(), [EnumMatchPatternFact::UnitLiteral { .. }]));

    let EnumMatchPatternFact::Enum(error) = &fact.arms[1].pattern else {
        panic!("Error arm must be an enum pattern");
    };
    let [EnumMatchPatternFact::ScalarLiteral(literal)] = error.items.as_ref() else {
        panic!("Error payload must retain its scalar literal");
    };
    assert_eq!(literal.semantic_type, SemanticTypeId::I64);
    assert_eq!(literal.value, LiteralFact::Integer(Arc::from("7_i64")));
    assert_eq!(fact.arms[2].pattern, EnumMatchPatternFact::Wildcard);
}

#[test]
fn enum_match_materializes_nested_nominal_enum_patterns_with_exact_layouts() {
    let source = "enum Inner { Value(i64 value), Empty } enum Outer { Wrap(Inner inner), None } bool Main(Outer outer) { return match outer { Outer::Wrap(Inner::Value(7_i64)) => true, Outer::Wrap(Inner::Empty) => false, Outer::None => false, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let inner_declaration = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let inner_layout = enum_layout(&db, inner_declaration).expect("inner layout query").expect("inner layout");
    let fact = enum_match(&db, expression).expect("enum match query").expect("nested enum patterns");

    let EnumMatchPatternFact::Enum(outer) = &fact.arms[0].pattern else {
        panic!("outer pattern must be nominal");
    };
    let [EnumMatchPatternFact::Enum(inner)] = outer.items.as_ref() else {
        panic!("outer payload must retain the nested enum pattern");
    };
    assert_eq!(inner.declaration, inner_declaration);
    assert_eq!(inner.layout, inner_layout);
    assert_eq!(inner.variant_index, 0);
    assert!(matches!(inner.items.as_ref(), [EnumMatchPatternFact::ScalarLiteral(_)]));
}

#[test]
fn enum_match_preserves_multi_field_payload_patterns_in_source_order() {
    let source = "enum Pair { Both(i64 number, i64 marker), Empty } i64 Main(Pair pair) { return match pair { Pair::Both(number, 7_i64) => number, Pair::Both(_, _) => 0_i64, Pair::Empty => -1_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let fact = enum_match(&db, expression).expect("enum match query").expect("multi-field enum match");

    let EnumMatchPatternFact::Enum(both) = &fact.arms[0].pattern else {
        panic!("Both arm must be an enum pattern");
    };
    let [EnumMatchPatternFact::Binding(number), EnumMatchPatternFact::ScalarLiteral(flag)] = both.items.as_ref() else {
        panic!("multi-field payload patterns must retain source order: {:?}", both.items);
    };
    assert_eq!(number.payload, AggregateFieldShape::Scalar(SemanticTypeId::I64));
    assert_eq!(flag.semantic_type, SemanticTypeId::I64);
    assert_eq!(flag.value, LiteralFact::Integer(Arc::from("7_i64")));
}

#[test]
fn enum_match_rejects_identifier_bindings_for_zero_sized_unit_payloads() {
    let source = "enum Result { Ok(unit value), Error(i64 error) } bool Main(Result result) { return match result { Result::Ok(value) => true, Result::Error(_) => false, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_unavailable(enum_match(&db, expression));
}
