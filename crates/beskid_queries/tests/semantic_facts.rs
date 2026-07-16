use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex, SyntaxSnapshot};
use beskid_queries::{
    AggregateFieldShape, AstNodeKey, BeskidDatabase, ClosureCapture, CompletionContext,
    ItemSignature, LocalSlot, OperatorFact, ProjectSession, SemanticError, SemanticTypeId,
    SourceUnitId, SyntaxGenerationId, aggregate_layout, build_typed_program, call_arguments,
    call_lowering, cast_intents, child_nodes, closure_environment, completion_candidates,
    control_flow, direct_callees, generic_call_instantiation, item_body, item_signature,
    literal_fact, local_slot, node_kind, node_span, node_type, operator_fact, reachable_items,
    resolved_item, resolved_local, runtime_intrinsic, spawn_target, test_item,
};

fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    let error = match result {
        Ok(_) => panic!("current unported semantic query must fail explicitly"),
        Err(error) => error,
    };
    assert!(error.is_unavailable(), "{error:?}");
}

fn setup(
    source: &str,
) -> (
    BeskidDatabase,
    ProjectSession,
    SourceUnitId,
    SyntaxGenerationId,
    SyntaxIndex,
) {
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
    let expanded = expand_program(
        parse_program(source).expect("parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let index = SyntaxIndex::from_program(&expanded, generation);
    db.ensure_file_text(unit.path(&db).clone(), source.to_string());
    db.ensure_syntax_unit(project, unit, generation)
        .expect("expanded syntax registration");
    (db, project, unit, generation, index)
}

#[test]
fn warm_point_query_uses_registered_expanded_syntax_without_reparse() {
    let (mut db, _project, unit, generation, index) = setup("i32 Main() { return 7; }");
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    assert!(literal_fact(&db, literal).expect("cold literal").is_some());
    assert_eq!(
        node_type(&db, literal).expect("cold type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );

    db.ensure_file_text(
        unit.path(&db).clone(),
        "this is deliberately invalid Beskid source".to_string(),
    );
    assert!(literal_fact(&db, literal).expect("warm literal").is_some());
    assert_eq!(
        node_type(&db, literal).expect("warm type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

#[test]
fn aggregate_layout_keeps_channel_options_nominal_capacity() {
    let source = "enum ChannelCapacity { Unbounded(), Bounded(i64 capacity) } type ChannelOptions { ChannelCapacity capacity, bool singleReader, bool singleWriter }";
    let (db, _project, unit, generation, index) = setup(source);
    let options = key(unit, generation, &index, NodeKind::TypeDefinition, 0);
    let capacity = key(unit, generation, &index, NodeKind::EnumDefinition, 0);
    let layout = aggregate_layout(&db, options)
        .expect("layout query")
        .expect("layout");
    assert_eq!(layout.fields.len(), 3);
    assert_eq!(layout.fields[0].0.as_ref(), "capacity");
    assert_eq!(layout.fields[0].1, AggregateFieldShape::Nominal(capacity));
    assert_eq!(
        layout.fields[1].1,
        AggregateFieldShape::Scalar(SemanticTypeId::BOOL)
    );
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
            .find(|metadata| {
                metadata.kind == kind && metadata.span.is_some_and(|span| span.start == start)
            })
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
        CompletionContext {
            cursor,
            replacement_start: cursor,
            replacement_end: cursor + 1,
        },
    )
    .expect("completion")
    .expect("current generation");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.label.as_ref())
            .collect::<Vec<_>>(),
        vec!["Zebra"]
    );
    assert_eq!(
        (
            candidates[0].replacement_start,
            candidates[0].replacement_end
        ),
        (cursor, cursor + 1)
    );
    assert_eq!(
        completion_candidates(
            &db,
            AstNodeKey {
                generation: SyntaxGenerationId(generation.0 - 1),
                ..program
            },
            CompletionContext {
                cursor,
                replacement_start: cursor,
                replacement_end: cursor
            }
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
            CompletionContext {
                cursor: invalid,
                replacement_start: invalid,
                replacement_end: invalid
            }
        ),
        Ok(None)
    );
}

#[test]
fn qualified_import_resolution_uses_registered_dependency_syntax() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/qualified-import/project/src");
    let main_path = root.join("Main.bd");
    let tools_path = root.join("Lib/Tools.bd");
    let main_source = "use Lib.Tools;\ni32 Main() { Tools.Member(); return Tools.Helper(); }";
    let tools_source = "i32 Helper() { return 1; }";
    let main_program = expand_program(
        parse_program(main_source).expect("main parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let tools_program = expand_program(
        parse_program(tools_source).expect("tools parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root.clone(),
            },
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
        main_source.find("Tools.Helper").expect("qualified call"),
    );
    let declaration = key(
        tools_unit,
        generation,
        &tools_index,
        NodeKind::FunctionDefinition,
        0,
    );

    assert_eq!(
        resolved_item(&db, reference).expect("qualified resolution"),
        Some(beskid_queries::ResolvedItem { declaration })
    );
    let call = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        0,
    );
    assert_eq!(
        call_lowering(&db, call).expect("qualified direct call"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
    let direct_call = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        1,
    );
    assert_eq!(
        call_lowering(&db, direct_call).expect("qualified direct call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    let main = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::FunctionDefinition,
        0,
    );
    let program = key(main_unit, generation, &main_index, NodeKind::Program, 0);
    assert_eq!(
        reachable_items(&db, program, main)
            .expect("cross-unit reachability")
            .expect("cross-unit graph")
            .as_ref(),
        &[main, declaration]
    );
    let member_cursor = main_source.find("Tools.Helper").expect("qualified call") + "Tools.".len();
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
    assert_eq!(
        members
            .iter()
            .map(|candidate| candidate.label.as_ref())
            .collect::<Vec<_>>(),
        vec!["Helper"]
    );
    assert_eq!(
        resolved_item(
            &db,
            AstNodeKey {
                generation: SyntaxGenerationId(16),
                ..reference
            }
        )
        .expect("stale generation"),
        None
    );
}

#[test]
fn unqualified_import_resolution_requires_one_registered_syntax_target() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/unqualified-import/project/src");
    let main_path = root.join("Main.bd");
    let tools_path = root.join("Lib/Tools.bd");
    let main_source = "use Lib.Tools;\ni32 Main() { return Helper(); }";
    let tools_source = "i32 Helper() { return 1; }";
    let main_program = expand_program(
        parse_program(main_source).expect("main parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let tools_program = expand_program(
        parse_program(tools_source).expect("tools parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root.clone(),
            },
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
    let call = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        0,
    );
    let helper = key(
        tools_unit,
        generation,
        &tools_index,
        NodeKind::FunctionDefinition,
        0,
    );

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
    let channel_source = "unit Create<T>() { return; }";
    let main_program = expand_program(
        parse_program(main_source).expect("main parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let channel_program = expand_program(
        parse_program(channel_source).expect("channel parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root.clone(),
            },
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
    let call = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        0,
    );
    let declaration = key(
        channel_unit,
        generation,
        &channel_index,
        NodeKind::FunctionDefinition,
        0,
    );

    assert_eq!(
        call_lowering(&db, call).expect("generic imported static call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
}

#[test]
fn generic_imported_terminal_call_requires_an_exact_declared_generic_arity() {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/generic-terminal-import/project/src");
    let main_path = root.join("Main.bd");
    let channel_path = root.join("Concurrency/Channel.bd");
    let main_source = "use Concurrency.Channel;\nunit Main() { Channel.CreateWithOptions<i64>(); Channel.CreateWithOptions<i64, i32>(); }";
    let channel_source = "unit CreateWithOptions<T>() { return; }";
    let main_program = expand_program(
        parse_program(main_source).expect("main parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let channel_program = expand_program(
        parse_program(channel_source).expect("channel parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root.clone(),
            },
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
    let call = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        0,
    );
    let declaration = key(
        channel_unit,
        generation,
        &channel_index,
        NodeKind::FunctionDefinition,
        0,
    );

    assert_eq!(
        call_lowering(&db, call).expect("generic imported terminal call"),
        Some(beskid_queries::CallLowering::Direct(declaration))
    );
    assert_eq!(
        generic_call_instantiation(&db, call).expect("exact generic instantiation"),
        Some(beskid_queries::GenericCallInstantiation {
            declaration,
            argument_count: 1,
        })
    );
    let mismatched = key(
        main_unit,
        generation,
        &main_index,
        NodeKind::CallExpression,
        1,
    );
    assert_eq!(
        call_lowering(&db, mismatched).expect("mismatched generic terminal call"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
    assert_eq!(
        generic_call_instantiation(&db, mismatched).expect("mismatched generic instantiation"),
        None
    );
}

#[test]
fn transparent_single_field_generic_types_have_only_source_derived_abi_facts() {
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
        beskid_queries::item_abi_signature(&db, create).expect("transparent signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([]),
            result: SemanticTypeId::I64,
        })
    );
    assert_eq!(
        beskid_queries::abi_type(&db, channel_call).expect("transparent call ABI"),
        Some(SemanticTypeId::I64)
    );
    assert_eq!(
        beskid_queries::abi_type(&db, channel_let).expect("transparent local ABI"),
        Some(SemanticTypeId::I64)
    );
    assert_unavailable(beskid_queries::item_abi_signature(&db, create_pair));
    assert_unavailable(beskid_queries::abi_type(&db, pair_call));
    assert_unavailable(beskid_queries::abi_type(&db, pair_let));
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

    assert_eq!(
        node_kind(&db, main).expect("kind"),
        Some(NodeKind::FunctionDefinition)
    );
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

    assert_eq!(
        node_type(&db, integer).expect("integer type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );
    assert_eq!(
        item_signature(&db, helper).expect("helper signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I64].into(),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        item_signature(&db, main).expect("main signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([]),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        control_flow(&db, main).expect("control flow"),
        Some(beskid_queries::ControlFlow {
            may_fall_through: false,
        })
    );
    assert!(
        resolved_local(&db, local_reference)
            .expect("local resolution")
            .is_some()
    );
    assert_eq!(
        resolved_item(&db, item_reference).expect("item resolution"),
        Some(beskid_queries::ResolvedItem {
            declaration: helper,
        })
    );
    assert_eq!(
        call_lowering(&db, call).expect("call lowering"),
        Some(beskid_queries::CallLowering::Direct(helper))
    );
    assert_unavailable(cast_intents(&db, call));
    assert_eq!(
        direct_callees(&db, main).expect("direct callees"),
        Some(Arc::from([helper]))
    );
    assert_eq!(
        reachable_items(&db, program, main).expect("reachable items"),
        Some(Arc::from([main, helper]))
    );
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
    assert_eq!(
        node_type(&db, input_reference).expect("input type"),
        Some(beskid_queries::SemanticTypeId::I64)
    );
    let expected = [
        beskid_queries::SemanticTypeId::BOOL,
        beskid_queries::SemanticTypeId::F64,
        beskid_queries::SemanticTypeId::STRING,
        beskid_queries::SemanticTypeId::CHAR,
        beskid_queries::SemanticTypeId::U8,
    ];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let literal = key(unit, generation, &index, NodeKind::Literal, occurrence);
        assert_eq!(
            node_type(&db, literal).expect("literal type"),
            Some(expected)
        );
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
    assert_eq!(
        cast_intents(&db, literal).expect("literal cast"),
        Some(Arc::clone(&expected))
    );
    assert_eq!(
        cast_intents(&db, source_reference).expect("local cast"),
        Some(expected)
    );

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
    let contract = key(
        unit,
        generation,
        &index,
        NodeKind::ContractMethodSignature,
        0,
    );

    assert_eq!(
        item_signature(&db, function).expect("function signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [
                beskid_queries::SemanticTypeId::I32,
                beskid_queries::SemanticTypeId::BOOL,
            ]
            .into(),
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
        Some(ItemSignature {
            parameters: Arc::from([]),
            result: SemanticTypeId::UNIT,
        })
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

    let facts = test_item(&db, test)
        .expect("test facts query")
        .expect("current test facts");
    assert_eq!(facts.name.as_ref(), "Smoke");
    assert_eq!(facts.qualified_name.as_ref(), "Smoke");
    assert_eq!(facts.group.as_deref(), Some("fast"));
    assert_eq!(
        facts
            .tags
            .iter()
            .map(|tag| tag.as_ref())
            .collect::<Vec<_>>(),
        ["unit", "smoke"]
    );
    assert_eq!(facts.skip_condition, Some(true));
    assert_eq!(facts.skip_reason.as_deref(), Some("not on this host"));
    assert_eq!(
        test_item(
            &db,
            AstNodeKey {
                generation: SyntaxGenerationId(generation.0 - 1),
                ..test
            }
        )
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

    assert_eq!(
        call_lowering(&db, call).expect("lambda call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
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
    let outer_call = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::CallExpression,
        outer_call_offset + "Target".len(),
    );
    let empty_call = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::CallExpression,
        empty_call_offset + "Target".len(),
    );
    let expected = [
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Expression,
            source.find("1,").expect("first argument"),
        ),
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Expression,
            nested_call_offset,
        ),
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Expression,
            source.find("value);").expect("value argument"),
        ),
    ];

    assert_eq!(
        call_arguments(&db, outer_call).expect("outer arguments"),
        Some(Arc::from(expected))
    );
    assert_eq!(
        call_arguments(&db, empty_call).expect("empty arguments"),
        Some(Arc::from([]))
    );
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_eq!(call_arguments(&db, main).expect("non-call"), None);

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(
        call_arguments(&db, outer_call).expect("stale arguments"),
        None
    );
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
    assert_eq!(
        call_lowering(&db, named_call).expect("named call"),
        Some(beskid_queries::CallLowering::Direct(helper))
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
    let leaf_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        leaf_call_offset,
    );
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
    assert_eq!(
        direct_callees(&db, recur).expect("recursive callees"),
        Some(Arc::from([recur]))
    );
    assert_eq!(
        direct_callees(&db, main).expect("main callees"),
        Some(Arc::from([leaf, recur]))
    );
    let reachable = reachable_items(&db, program, main)
        .expect("reachable query")
        .expect("reachable facts");
    assert_eq!(reachable.as_ref(), &[main, leaf, recur]);
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
    let shadowed_path = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        shadowed_offset,
    );
    let shadowed_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_eq!(
        resolved_item(&db, shadowed_path).expect("shadowed item"),
        None
    );
    assert_unavailable(call_lowering(&db, shadowed_call));

    let unresolved_source = "i32 Main() { return Missing(); }";
    let (db, _project, unit, generation, index) = setup(unresolved_source);
    let unresolved_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let unresolved_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let unresolved_main = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let unresolved_program = key(unit, generation, &index, NodeKind::Program, 0);
    assert_eq!(
        resolved_item(&db, unresolved_path).expect("unresolved item"),
        None
    );
    assert_unavailable(call_lowering(&db, unresolved_call));
    assert_unavailable(direct_callees(&db, unresolved_main));
    assert_unavailable(reachable_items(&db, unresolved_program, unresolved_main));
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
        Some(beskid_queries::ResolvedItem {
            declaration: inner_helper,
        })
    );
    assert_eq!(
        resolved_item(&db, outer_only_path).expect("outer module fallback"),
        Some(beskid_queries::ResolvedItem {
            declaration: outer_only,
        })
    );
}

#[test]
fn stale_generation_cannot_reuse_item_or_call_graph_facts() {
    let source = "i32 Helper() { return 1; } i32 Main() { return Helper(); }";
    let (mut db, project, unit, generation, index) = setup(source);
    let helper_path = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert!(
        resolved_item(&db, helper_path)
            .expect("current item")
            .is_some()
    );
    assert!(
        direct_callees(&db, main)
            .expect("current callees")
            .is_some()
    );

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
        Some(beskid_queries::ControlFlow {
            may_fall_through: false,
        })
    );
    assert_eq!(
        control_flow(&db, may_fall_through).expect("fall-through flow"),
        Some(beskid_queries::ControlFlow {
            may_fall_through: true,
        })
    );
}

#[test]
fn stale_generation_never_observes_semantic_facts() {
    let (mut db, project, unit, generation, index) = setup("i32 Main() { return 0; }");
    let current = key(unit, generation, &index, NodeKind::Literal, 0);
    assert_eq!(
        node_type(&db, current).expect("current type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );

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

    assert!(
        resolved_local(&db, first_reference)
            .expect("first local")
            .is_some()
    );
    assert_eq!(
        resolved_local(&db, second_reference).expect("out-of-scope local"),
        None
    );
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
    let value_offsets = source
        .match_indices("value")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(value_offsets.len(), 5);

    let parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        value_offsets[0],
    );
    let inner_declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        value_offsets[2],
    );
    let parameter_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[1],
    );
    let inner_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[3],
    );
    let outer_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[4],
    );

    assert_eq!(
        resolved_local(&db, parameter_reference).expect("parameter reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: parameter,
        })
    );
    assert_eq!(
        resolved_local(&db, inner_reference).expect("shadowed reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: inner_declaration,
        })
    );
    assert_eq!(
        resolved_local(&db, outer_reference).expect("outer reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: parameter,
        })
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
        let offsets = source
            .match_indices(binding_name)
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 2, "{binding_name} occurrences in {source}");
        let declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, offsets[0]);
        let reference = key_at_start(
            unit,
            generation,
            &index,
            NodeKind::PathExpression,
            offsets[1],
        );
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
    let offsets = source
        .match_indices("value")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let initializer_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        offsets[1],
    );
    assert_eq!(
        resolved_local(&db, initializer_reference).expect("initializer local"),
        None
    );
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
    let value_offsets = source
        .match_indices("value")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let outer_offsets = source
        .match_indices("outer")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let declarations = [
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            value_offsets[0],
        ),
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            outer_offsets[0],
        ),
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            outer_offsets[1],
        ),
        key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            source.find("apply").expect("apply declaration"),
        ),
    ];
    for (slot_index, declaration) in declarations.into_iter().enumerate() {
        assert_eq!(
            local_slot(&db, declaration).expect("function local slot"),
            Some(LocalSlot {
                owner,
                index: u32::try_from(slot_index).expect("slot index"),
            })
        );
    }

    let inner = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("inner").expect("lambda parameter"),
    );
    assert_eq!(
        local_slot(&db, inner).expect("lambda local slot"),
        Some(LocalSlot {
            owner: lambda_owner,
            index: 0,
        })
    );
    let function_name = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("Main").expect("function name"),
    );
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
    let copied = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        copied_offset,
    );
    let inner_offset = source.find("inner) =>").expect("lambda parameter");
    let inner = key_at_start(unit, generation, &index, NodeKind::Identifier, inner_offset);

    let closure = closure_environment(&db, lambda)
        .expect("closure environment")
        .expect("lambda fact");
    assert_eq!(closure.parameters.as_ref(), &[inner]);
    assert_eq!(
        closure.captures.as_ref(),
        &[ClosureCapture {
            declaration: copied,
            slot: local_slot(&db, copied)
                .expect("outer local slot")
                .expect("outer local slot fact"),
        }]
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

    let spawn = spawn_target(&db, spawn)
        .expect("spawn target")
        .expect("spawn fact");
    assert_eq!(spawn.callee, lambda);
    assert_eq!(
        spawn.captures.as_ref(),
        &[ClosureCapture {
            declaration: outer,
            slot: local_slot(&db, outer)
                .expect("parameter slot")
                .expect("parameter slot fact"),
        }]
    );
}

#[test]
fn runtime_intrinsic_uses_the_manifest_owned_builtin_index() {
    let source = "i32 Main() { __str_len(\"value\"); return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let expected = beskid_analysis::builtins::builtin_for_path(&["__str_len".to_string()])
        .expect("generated builtin")
        .0;

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
fn syntax_only_signatures_preserve_runtime_pointer_and_never_primitives() {
    let source = "pointer Echo(pointer value) { return value; } never Stop() { while true {} }";
    let (db, _project, unit, generation, index) = setup(source);
    let echo = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let stop = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        item_signature(&db, echo).expect("pointer signature"),
        Some(ItemSignature {
            parameters: Arc::from([SemanticTypeId::POINTER]),
            result: SemanticTypeId::POINTER,
        })
    );
    assert_eq!(
        item_signature(&db, stop).expect("never signature"),
        Some(ItemSignature {
            parameters: Arc::from([]),
            result: SemanticTypeId::NEVER,
        })
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
            Some(LocalSlot {
                owner: method,
                index: expected_index,
            })
        );
    }

    for (source, declarations) in [
        (
            "unit Main() { for item in [1] { let copy = item; } }",
            [("item", 0), ("copy", 1)],
        ),
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
                Some(LocalSlot {
                    owner,
                    index: expected_index,
                }),
                "binding {name} in {source}"
            );
        }
    }
}

#[test]
fn stale_generation_cannot_reuse_a_local_slot_identity() {
    let source = "i32 Main() { let value = 1; return value; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("value").expect("local declaration"),
    );
    let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert!(
        resolved_local(&db, reference)
            .expect("current local")
            .is_some()
    );
    assert_eq!(
        local_slot(&db, declaration).expect("current local slot"),
        Some(LocalSlot { owner, index: 0 })
    );

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
fn operator_facts_cover_expression_selection() {
    let source = "bool Main() { let value = 1 + 2; return !(value == 3); }";
    let (db, _project, unit, generation, index) = setup(source);
    let add = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let equals = key(unit, generation, &index, NodeKind::BinaryExpression, 1);
    let not = key(unit, generation, &index, NodeKind::UnaryExpression, 0);

    assert_eq!(
        operator_fact(&db, add).expect("operator"),
        Some(OperatorFact::Add)
    );
    assert_eq!(
        operator_fact(&db, equals).expect("operator"),
        Some(OperatorFact::Eq)
    );
    assert_eq!(
        operator_fact(&db, not).expect("operator"),
        Some(OperatorFact::Not)
    );
}

#[test]
fn item_body_is_the_exact_function_and_method_body_child() {
    let function_source = "i32 Main() { return 0; }";
    let (function_db, _project, unit, generation, index) = setup(function_source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let function_program = expand_program(
        parse_program(function_source).expect("function parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let function_snapshot = SyntaxSnapshot::from_program(&function_program, generation.0);
    let function_node = function_snapshot
        .node_at(function.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .expect("function definition");
    let expected_function_body = function_snapshot
        .id_of(DynNodeRef::from(&function_node.body))
        .expect("exact function body");
    assert_eq!(
        item_body(&function_db, function).expect("function body"),
        Some(AstNodeKey {
            node: beskid_analysis::syntax::AstNodeId(expected_function_body),
            ..function
        })
    );

    let method_source = "type Value { i32 raw } impl Value { i32 Get() { return this.raw; } }";
    let (method_db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let method_program = expand_program(
        parse_program(method_source).expect("method parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let method_snapshot = SyntaxSnapshot::from_program(&method_program, generation.0);
    let method_node = method_snapshot
        .node_at(method.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
        .expect("method definition");
    let expected_method_body = method_snapshot
        .id_of(DynNodeRef::from(&method_node.body))
        .expect("exact method body");
    assert_eq!(
        item_body(&method_db, method).expect("method body"),
        Some(AstNodeKey {
            node: beskid_analysis::syntax::AstNodeId(expected_method_body),
            ..method
        })
    );
}
