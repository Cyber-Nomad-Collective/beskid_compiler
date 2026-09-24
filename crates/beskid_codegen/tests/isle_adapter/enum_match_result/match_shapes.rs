//! Nominal, nested, multi-field, literal, and guarded match pattern shapes.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput, DirectCallee,
    EffectiveCompilationRoots, HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, ModuleIndex,
    NodeFacts, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId,
    SyntaxModuleItem, TargetMetadata, build_typed_program, default_libcall_names, emit_isle_item,
    emit_isle_item_with_call_importer, enum_constructor, enum_match, find_call_expression, find_function_definition,
    find_function_definitions, find_node, find_nodes_of_kind, isa, item_fixture, item_fixture_with_root, item_name,
    lower_syntax_program, node_type, parse_program_with_source_name, settings,
};

fn assert_enum_match_shape_remains_unavailable(source: &str) {
    let (input, _isa, root) = item_fixture_with_root(source);
    let expression =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).expect("match expression");
    let error =
        enum_match(input.database(), expression).expect_err("unsupported enum-match shape must remain unavailable");
    assert!(error.is_unavailable(), "{error:?}");
}

#[test]
fn nominal_enum_parameter_materializes_as_a_pointer_local_slot() {
    let (input, isa, item) =
        item_fixture("enum StandardStream { Stdin, Stdout, Stderr } unit Main(StandardStream stream) { return; }");
    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("a nominal parameter must materialize as an emitter local");

    assert_eq!(
        function.signature.params[0].value_type,
        isa.pointer_type(),
        "a nominal enum value is represented by the target pointer type while the emitter materializes its local slot"
    );
}

#[test]
fn nested_nominal_enum_payload_binding_lowers_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } i64 Main(Descriptor descriptor) { return match descriptor { Descriptor::Standard(stream) => match stream { StandardStream::Stdin => 0_i64, StandardStream::Stdout => 1_i64, StandardStream::Stderr => 2_i64, }, Descriptor::Raw(fd) => fd, }; }",
    );

    if let Err(error) = emit_isle_item(&input, isa.as_ref(), item) {
        panic!(
            "nested nominal enum payload bindings must lower through syntax facts: {}",
            error.display_with_db(input.database())
        );
    }
}

#[test]
fn nominal_match_payload_field_projection_lowers_from_authoritative_binding_layout() {
    let (input, isa, item) = item_fixture(
        "type ParseSuccess<T> { T value, i64 rest } enum TextParseResult<T> { Ok(ParseSuccess<T> success), Err(i64 error) } i64 Main(TextParseResult<string> result) { return match result { TextParseResult::Ok(success) => success.rest, TextParseResult::Err(_) => -1_i64, }; }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("a pointer-applied nominal match payload must retain its aggregate layout for field projection");
}

#[test]
fn scalar_applied_nominal_match_payload_projects_the_specialized_value_layout() {
    let (input, isa, item) = item_fixture(
        "type ParseSuccess<T> { T value, i64 rest } enum TextParseResult<T> { Ok(ParseSuccess<T> success), Err(i64 error) } i32 Main(TextParseResult<i32> result) { return match result { TextParseResult::Ok(success) => success.value, TextParseResult::Err(_) => -1, }; }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("a scalar-applied nominal match payload must project the concrete scalar field layout");
}

#[test]
fn unspecialized_generic_parameter_remains_unavailable_for_local_materialization() {
    let (input, _isa, item) = item_fixture("unit Identity<T>(T value) { return; }");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    assert_eq!(
        facts.function_parameters(item),
        None,
        "a generic parameter without an item specialization must not invent a local ABI type"
    );
}

#[test]
fn enum_match_scalar_literal_payload_emits_an_explicit_comparison() {
    let (input, isa, item) = item_fixture(
        "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(7_i64) => 1_i64, Result::Ok(_) => 2_i64, Result::Error(_) => 0_i64, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("literal-payload enum match");
    assert!(
        enum_match(input.database(), expression).expect("literal-payload enum match query").is_some(),
        "a supported scalar literal must become an explicit arm predicate"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a scalar literal payload lowers as a tag dispatch followed by equality comparison");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp eq"), "the literal arm must compare the loaded payload: {clif}");
    assert!(clif.contains("iconst.i64 7"), "the literal comparison must retain the source value: {clif}");
}

#[test]
fn enum_match_expression_accepts_a_never_returning_arm() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { Value(i64 value), Fatal } i64 Main(Choice choice) { return match choice { Choice::Value(value) => value, Choice::Fatal => Fail(), }; } never Fail() { return Fail(); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let item = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main item");
    let fail = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Fail"))
        .expect("Fail item");
    let expression =
        find_node(db, item, beskid_queries::IndexedNodeKind::MatchExpression).expect("match expression with never arm");

    assert_eq!(
        node_type(db, expression).expect("match result type"),
        Some(beskid_queries::SemanticTypeId::I64),
        "a never-returning arm must not replace the concrete match result type"
    );
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    let imported = module.declare_function("Fail", Linkage::Import, &signature).expect("declare never callee");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(fail), imported)]));
    if let Err(error) = emit_isle_item_with_call_importer(&input, isa.as_ref(), item, &mut importer) {
        panic!("a never-returning arm terminates without supplying a merge value: {}", error.display_with_db(db));
    }
}

#[test]
fn enum_match_nested_nominal_enum_pattern_recurses_in_source_order() {
    let (input, isa, item) = item_fixture(
        "enum Inner { Value(i64 value), Other() } enum Result { Ok(Inner value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(Inner::Value(7_i64)) => 1_i64, Result::Ok(_) => 2_i64, Result::Error(_) => 0_i64, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("nested nominal enum match");
    assert!(
        enum_match(input.database(), expression).expect("nested nominal enum match query").is_some(),
        "nested enum constructors must remain explicit in the recursive semantic pattern"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("nested nominal enum patterns lower through ordered tag and payload tests");
    let clif = function.display().to_string();
    assert!(
        clif.matches("icmp eq").count() >= 3,
        "outer tag, nested tag, and scalar leaf must each be compared explicitly: {clif}"
    );
    assert!(clif.contains("iconst.i64 7"), "the nested literal must retain its source value: {clif}");
}

#[test]
fn cross_unit_generic_receiver_match_preserves_a_concrete_nominal_error() {
    let mut db = Box::new(BeskidDatabase::default());
    let project_root = tempfile::tempdir().expect("project").keep();
    let source_root = project_root.join("src");
    let main_path = source_root.join("Main.bd");
    let fiber_path = source_root.join("Concurrency/Work.bd");
    let fiber_error_path = source_root.join("Concurrency/WorkError.bd");
    let fiber_status_path = source_root.join("Concurrency/WorkStatus.bd");
    let results_path = source_root.join("Core/Results.bd");
    let main_source = "use Concurrency.Work; unit Main() { Work<i64> fiber = Work<i64> { value: 7_i64, handle: -1_i64 }; Work<i64>.Join(fiber); return; }";
    let fiber_source = "use Concurrency.WorkError; use Concurrency.WorkStatus; use Core.Results; pub type Work<T> { T value, i64 handle } pub Core.Results.Result<T, WorkError> Join<T>(Work<T> self) { WorkStatus status = WorkStatus::Panicked(self.handle, \"panic\"); return match status { WorkStatus::Ok(_) => Result::Ok(self.value), WorkStatus::Cancelled(reason, cancelerId) => Result::Error(WorkError::Cancelled(reason, cancelerId)), WorkStatus::StackOverflow(limitBytes, requestedBytes) => Result::Error(WorkError::StackOverflow(limitBytes, requestedBytes)), WorkStatus::Panicked(code, message) => Result::Error(WorkError::Panicked(code, message)), WorkStatus::NotDone => Result::Error(WorkError::Panicked(-1_i64, \"not done\")), }; }";
    let fiber_error_source = "pub enum WorkError { Cancelled(i64 reason, i64 cancelerId), StackOverflow(i64 limitBytes, i64 requestedBytes), Panicked(i64 code, string message) }";
    let fiber_status_source = "pub enum WorkStatus { Ok(i64 value), Cancelled(i64 reason, i64 cancelerId), StackOverflow(i64 limitBytes, i64 requestedBytes), Panicked(i64 code, string message), NotDone }";
    let results_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    std::fs::create_dir_all(fiber_path.parent().expect("fiber parent")).expect("create source tree");
    std::fs::create_dir_all(results_path.parent().expect("results parent")).expect("create Core source tree");
    std::fs::write(&main_path, main_source).expect("write Main source");
    std::fs::write(&fiber_path, fiber_source).expect("write Work source");
    std::fs::write(&fiber_error_path, fiber_error_source).expect("write WorkError source");
    std::fs::write(&fiber_status_path, fiber_status_source).expect("write WorkStatus source");
    std::fs::write(&results_path, results_source).expect("write Results source");
    let units = [
        (main_path.clone(), main_source),
        (fiber_path.clone(), fiber_source),
        (fiber_error_path, fiber_error_source),
        (fiber_status_path, fiber_status_source),
        (results_path, results_source),
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
    let generation = SyntaxGenerationId(149);
    let entry = SourceUnitId::new(&*db, main_path.clone());
    let fiber_unit = SourceUnitId::new(&*db, fiber_path);
    let project = ProjectSession::new(&*db, project_root, main_path, "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::from(units.clone()),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed cross-unit program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let main = find_function_definition(&*db, root).expect("Main definition");
    let fiber_index = beskid_analysis::syntax_query::SyntaxIndex::from_program(&units[1].program, generation);
    let join = AstNodeKey {
        unit: fiber_unit,
        generation,
        node: fiber_index
            .ids_of_kind(beskid_queries::IndexedNodeKind::FunctionDefinition)
            .next()
            .expect("Join definition"),
    };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("cross-unit codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let call = find_call_expression(input.database(), main).expect("Work<i64>.Join call");
    let specialization = beskid_queries::generic_call_specialization(input.database(), call)
        .expect("Join specialization query")
        .expect("the applied Work receiver must specialize Join<T>");
    let matched =
        find_node(input.database(), join, beskid_queries::IndexedNodeKind::MatchExpression).expect("Join status match");
    assert!(
        beskid_queries::enum_match_specialization(input.database(), matched, specialization.substitutions.clone(),)
            .expect("specialized Join match query")
            .is_some(),
        "the imported concrete WorkError must survive beside the owner-propagated T"
    );
    for constructor in
        find_nodes_of_kind(input.database(), join, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
    {
        let ordinary = enum_constructor(input.database(), constructor).ok().flatten();
        let specialized = beskid_queries::enum_constructor_specialization(
            input.database(),
            constructor,
            specialization.substitutions.clone(),
        )
        .expect("specialized Join constructor query");
        assert!(ordinary.is_some() || specialized.is_some(), "every Join enum constructor must retain a layout");
    }

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: join, symbol: "Join".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("generic Result constructor arms must retain their concrete nominal error payload");
}

#[test]
fn enum_match_accepts_collectively_exhaustive_nested_nominal_variants() {
    let (input, isa, item) = item_fixture(
        "enum Inner { Value(i64 value), Other() } enum Result { Ok(Inner value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(Inner::Value(_)) => 1_i64, Result::Ok(Inner::Other()) => 2_i64, Result::Error(_) => 0_i64, }; }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("all nested nominal variants collectively make their outer variant exhaustive");
}

#[test]
fn enum_match_lowers_multi_field_payload_patterns_in_source_order() {
    let (input, isa, item) = item_fixture(
        "enum Pair { Both(i64 number, i64 marker), Empty } i64 Main(Pair pair) { return match pair { Pair::Both(_, 7_i64) => 1_i64, Pair::Both(number, _) => number, Pair::Empty => 0_i64, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("multi-field enum match");
    let fact = enum_match(input.database(), expression)
        .expect("multi-field enum match query")
        .expect("multi-field enum match fact");
    let beskid_queries::EnumMatchPatternFact::Enum(first) = &fact.arms[0].pattern else {
        panic!("Both arm must remain nominal");
    };
    assert_eq!(first.items.len(), 2, "both source payload fields must reach lowering");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("multi-field enum payload patterns lower as ordered field predicates");
    let clif = function.display().to_string();
    assert!(clif.matches("load.i64").count() >= 2, "both integer payload fields must be loaded: {clif}");
}

#[test]
fn regex_span_list_match_lowers_two_nominal_payload_fields() {
    let (input, isa, item) = item_fixture(
        "type MatchSpan { i64 start, i64 end } enum MatchSpanList { Nil, Cons(MatchSpan head, MatchSpanList tail) } bool Main(MatchSpanList list) { return match list { MatchSpanList::Nil => true, MatchSpanList::Cons(_, _) => false, }; }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("the Regex MatchSpanList::Cons(_, _) pattern lowers with both nominal payload fields");
}

#[test]
fn regex_span_list_constructor_stores_two_nominal_payload_fields_in_source_order() {
    let (input, isa, item) = item_fixture(
        "type MatchSpan { i64 start, i64 end } enum MatchSpanList { Nil, Cons(MatchSpan head, MatchSpanList tail) } MatchSpanList Main(MatchSpan head, MatchSpanList tail) { return MatchSpanList::Cons(head, tail); }",
    );
    let constructor = find_node(input.database(), item, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
        .expect("Regex MatchSpanList::Cons constructor");
    let fact = enum_constructor(input.database(), constructor)
        .expect("multi-field enum constructor query")
        .expect("multi-field enum constructor fact");
    assert_eq!(fact.payloads.len(), 2, "both Regex payload expressions must remain source ordered");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("the Regex MatchSpanList::Cons(head, tail) constructor lowers both nominal payload fields");
    let clif = function.display().to_string();
    assert!(clif.matches("store ").count() >= 3, "the tag and both nominal payload fields must be stored: {clif}");
    let facts = beskid_codegen::SyntaxNodeFacts::new_with_isa(&input, isa.as_ref());
    let layout = NodeFacts::enum_layout(&facts, constructor).expect("physical MatchSpanList layout");
    let fields = &layout.variants[1].payload_fields;
    let parameters =
        function.layout.entry_block().map(|block| function.dfg.block_params(block)).expect("Main entry block");
    assert_eq!(fields.len(), parameters.len(), "each constructor parameter must retain one payload slot");
    for (field, parameter) in fields.iter().zip(parameters) {
        let field = field.as_ref().expect("both Regex payloads are managed references");
        let offset = i64::from(field.offset);
        let store = function
            .layout
            .blocks()
            .flat_map(|block| function.layout.block_insts(block))
            .find(|instruction| {
                matches!(
                    &function.dfg.insts[*instruction],
                    cranelift_codegen::ir::InstructionData::Store { offset: instruction_offset, .. }
                        if i64::from(*instruction_offset) == offset
                )
            })
            .expect("source-ordered payload store");
        assert_eq!(
            function.dfg.inst_args(store).first(),
            Some(parameter),
            "payload store order must match source order"
        );
    }
}

#[test]
fn enum_match_recursively_covers_a_nested_field_in_a_multi_field_payload() {
    let (input, isa, item) = item_fixture(
        "enum Bit { Zero, One } enum Pair { Both(Bit left, Bit right), Empty } i64 Main(Pair pair) { return match pair { Pair::Both(Bit::Zero, _) => 0_i64, Pair::Both(Bit::One, _) => 1_i64, Pair::Empty => 2_i64, }; }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("collective nested coverage remains exhaustive inside a multi-field payload");
}

#[test]
fn enum_match_guarded_binding_pattern_remains_unavailable() {
    assert_enum_match_shape_remains_unavailable(
        "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(value) when value > 0_i64 => 1_i64, Result::Error(_) => 0_i64, }; }",
    );
}
