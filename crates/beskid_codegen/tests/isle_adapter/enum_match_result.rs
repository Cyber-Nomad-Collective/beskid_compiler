use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput, DirectCallee,
    EffectiveCompilationRoots, HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, ModuleIndex,
    NodeFacts, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId,
    SyntaxModuleItem, TargetMetadata, build_typed_program, call_abi_signature, call_lowering, default_libcall_names,
    emit_isle_expression, emit_isle_item, emit_isle_item_with_call_importer, enum_constructor, enum_layout, enum_match,
    find_call_expression, find_function_definition, find_function_definitions, find_node, find_nodes_of_kind,
    find_test_definition, isa, item_body, item_fixture, item_fixture_with_root, item_name, lower_syntax_program,
    node_type, parse_program_with_source_name, settings, types,
};

#[test]
fn parsed_enum_constructor_uses_source_layout_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "enum Choice { None(), Some(i32 value) } i32 Main() { Choice choice = Choice::Some(7); return 0; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source).expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let constructor =
        find_node(&db, root, beskid_queries::IndexedNodeKind::EnumConstructorExpression).expect("enum constructor");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("enum constructor lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(input.enum_static_plan(constructor).is_some(), "enum static plan");
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"));
    assert!(!clif.contains("stack_store"));
    assert!(clif.contains("iconst.i32 1"));
}

#[test]
fn parsed_generic_enum_constructor_uses_concrete_source_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(7_i64); return 0; }",
    );
    let constructor = find_node(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
        .expect("generic enum constructor");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("generic enum constructor lowers from its concrete use-site layout");

    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    assert!(!clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("iconst.i32 0"), "{clif}");
    assert!(clif.contains("iconst.i64 7"), "{clif}");
}

#[test]
fn contextual_generic_result_constructor_accepts_a_nested_nominal_error() {
    let (input, isa, root) = item_fixture_with_root(
        "enum EnvironmentError { InvalidName(string name), NotFound(string name) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<string, EnvironmentError> Main(string name) { return Result::Error(EnvironmentError::InvalidName(name)); }",
    );
    let main = find_function_definition(input.database(), root).expect("Main definition");

    emit_isle_item(&input, isa.as_ref(), main)
        .expect("the return type supplies the generic Result layout for its nested nominal error constructor");
}

#[test]
fn mixed_pointer_scalar_generic_enum_uses_variant_specific_payload_slots() {
    let (input, isa, item) = item_fixture(
        "enum SyscallError { InvalidFd(i64 fd) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i64, SyscallError> Main(SyscallError error) { Result<i64, SyscallError> result = Result<i64, SyscallError>::Error(error); return result; }",
    );
    let constructor = find_node(input.database(), item, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
        .expect("Result::Error constructor");
    let plan = input.enum_static_plan(constructor).expect("mixed enum static plan");
    let pointer_slot = plan
        .fields
        .iter()
        .find(|field| field.abi_type == beskid_queries::SemanticTypeId::POINTER)
        .expect("dedicated pointer payload slot");
    assert_eq!(
        plan.pointer_map_offsets.as_ref(),
        &[pointer_slot.field_offset],
        "the applied nominal payload must be the enum's sole traced slot"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("mixed pointer/scalar generic enum must lower through variant-specific physical slots");
    let clif = function.display().to_string();

    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    let error_parameter = function
        .layout
        .entry_block()
        .and_then(|block| function.dfg.block_params(block).first().copied())
        .expect("Main error parameter");
    assert_eq!(function.dfg.value_type(error_parameter), isa.pointer_type());

    let pointer_offset = i64::try_from(pointer_slot.field_offset).expect("pointer payload offset fits CLIF");
    let pointer_store = function
        .layout
        .blocks()
        .flat_map(|block| function.layout.block_insts(block))
        .find(|instruction| {
            matches!(
                &function.dfg.insts[*instruction],
                cranelift_codegen::ir::InstructionData::Store { offset, .. }
                    if i64::from(*offset) == pointer_offset
            )
        })
        .expect("Result::Error payload store at the traced pointer slot");
    let [stored_value, _object] = function.dfg.inst_args(pointer_store) else {
        panic!("pointer payload store must have value and object operands: {clif}");
    };
    assert_eq!(
        *stored_value, error_parameter,
        "the nominal error parameter must be stored in the traced pointer slot: {clif}"
    );
}

#[test]
fn unsuffixed_integer_enum_payload_uses_declared_i64_layout() {
    let (input, isa, root) = item_fixture_with_root(
        "enum ReadLimit { UpTo(i64 maxBytes), Default } i64 Main() { ReadLimit limit = ReadLimit::UpTo(1); return 0; }",
    );
    let constructor = find_node(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
        .expect("ReadLimit::UpTo constructor");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("the declared enum payload width must authorize the unsuffixed integer literal");

    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 1"), "{clif}");
}

#[test]
fn parsed_result_try_lowers_to_verified_syntax_isle_control_flow() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i32, Error> Main(Result<i32, Error> value) { i32 output = value?; return Result::Ok(output); }",
    );
    let item = find_function_definition(input.database(), root).expect("Main definition");

    let function =
        emit_isle_item(&input, isa.as_ref(), item).expect("Result propagation lowers through generated syntax ISLE");
    let clif = function.display().to_string();

    assert!(clif.contains("brif"), "try propagation must branch on the Result discriminant: {clif}");
    assert!(!clif.contains("call_indirect"), "try propagation must not dynamically dispatch: {clif}");
    assert!(!clif.contains("beskid_rt_v5_result"), "try propagation must not import a result runtime helper: {clif}");
}

#[test]
fn parsed_result_try_rejects_noncanonical_result_definition_before_clif() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Err(TError error) } Result<i32, Error> Main(Result<i32, Error> value) { i32 output = value?; return Result::Ok(output); }",
    );
    let item = find_function_definition(input.database(), root).expect("Main definition");

    let error = emit_isle_item(&input, isa.as_ref(), item)
        .expect_err("a Result lookalike must be rejected before generated CLIF emission");
    let rendered = error.display_with_db(input.database());

    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("TryExpression@"), "{rendered}");
}

#[test]
fn parsed_nullary_enum_constructor_uses_source_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { None(), Some(i32 value) } i32 Main() { Choice choice = Choice::None(); return 0; }",
    );
    let constructor = find_node(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
        .expect("enum constructor");

    let function = emit_isle_expression(&input, isa.as_ref(), constructor, isa.pointer_type())
        .expect("nullary enum constructor lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"));
    assert!(!clif.contains("stack_store"));
    assert!(clif.contains("iconst.i32 0"));
}

#[test]
fn parsed_enum_match_uses_source_arms_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { None(), Some() } i32 Main() { return match Choice::Some() { Choice::None() => 1, Choice::Some() => 2, }; }",
    );
    let expression =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).expect("enum match");
    assert!(enum_match(input.database(), expression).expect("enum match query").is_some(), "source match facts");
    assert_eq!(node_type(input.database(), expression).expect("match type"), Some(beskid_queries::SemanticTypeId::I32));
    let function = emit_isle_expression(&input, isa.as_ref(), expression, types::I32)
        .expect("enum match lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"));
    assert_eq!(clif.matches("brif").count(), 2, "each source arm must retain its ordered tag test: {clif}");
}

#[test]
fn parsed_generic_enum_match_uses_explicit_scrutinee_layout_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64, string> value = Result<i64, string>::Ok(7_i64); return match value { Result::Ok(_) => 1_i64, Result::Error(_) => 0_i64, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("generic enum match");
    assert!(
        enum_match(input.database(), expression).expect("generic enum match query").is_some(),
        "generic match semantic facts"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("generic enum match lowers through its explicit source layout");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("iconst.i64 1"), "{clif}");
}

#[test]
fn generic_result_predicate_match_uses_each_call_specialization_for_its_scrutinee() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } bool Main(Result<i64, string> left, Result<string, i64> right) { return IsOk<i64, string>(left) && IsOk<string, i64>(right); }",
    );
    let items = find_function_definitions(input.database(), root);
    let is_ok = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk definition");
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a generic predicate match must consume the same call specialization as its parameter local");
}

#[test]
fn concrete_array_result_match_survives_a_sibling_generic_result_specialization() {
    let (input, isa, root) = item_fixture_with_root(
        "enum EncodingError { Invalid() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } u8[] StringToBytes(Result<u8[], EncodingError> encoded, u8[] empty) { return match encoded { Result::Ok(bytes) => bytes, Result::Error(_) => empty, }; } unit Main(Result<u8[], EncodingError> encoded, u8[] empty) { IsOk<u8[], EncodingError>(encoded); u8[] bytes = StringToBytes(encoded, empty); return; }",
    );
    let definitions = find_function_definitions(input.database(), root);
    let string_to_bytes = definitions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("StringToBytes"))
        .expect("StringToBytes definition");
    let expression = find_node(input.database(), string_to_bytes, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("StringToBytes match");
    assert!(
        enum_match(input.database(), expression).expect("concrete Result match query").is_some(),
        "the concrete applied Result type must retain its array payload ownership"
    );
    let items = definitions
        .into_iter()
        .map(|item| SyntaxModuleItem {
            symbol: item_name(input.database(), item).expect("item name query").expect("named function").to_string(),
            key: item,
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a generic sibling specialization must not shadow a concrete array Result match");
}

#[test]
fn parsed_generic_unit_payload_pattern_lowers_without_fabricating_storage() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool Main() { Result<unit, string> value = Result<unit, string>::Ok(()); return match value { Result::Ok(()) => true, Result::Error(_) => false, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("generic unit-payload enum match");
    assert!(
        enum_match(input.database(), expression).expect("generic unit-payload match query").is_some(),
        "the canonical unit pattern must be represented as a payload-free variant test"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a unit payload pattern lowers through the variant tag without a payload load");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "the match must load the variant tag: {clif}");
    assert!(!clif.contains("load.i8"), "the zero-sized unit payload must not be loaded: {clif}");
}

#[test]
fn generic_enum_constructor_without_context_remains_unavailable() {
    let (input, _isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } enum SyscallError { InvalidFd(i64 fd) } unit Main() { Result::Error(SyscallError::InvalidFd(1_i64)); return; }",
    );
    let constructor =
        find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
            .into_iter()
            .find(|key| enum_constructor(input.database(), *key).is_err())
            .expect("genericless Result::Error constructor");
    let error = enum_constructor(input.database(), constructor)
        .expect_err("uncontextualized generic enum constructor must remain unavailable");
    assert!(error.is_unavailable(), "{error:?}");
}

#[test]
fn generic_enum_constructor_uses_its_explicit_typed_let_context() {
    let (input, _isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result::Error(7_i64); return; }",
    );
    let constructor =
        find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
            .into_iter()
            .next()
            .expect("Result::Error constructor");
    assert!(
        enum_constructor(input.database(), constructor).expect("typed-let constructor query").is_some(),
        "explicit typed-let context must supply the generic Result arguments"
    );
}

#[test]
fn generic_enum_constructor_uses_its_declared_return_context() {
    let (input, _isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i64, i64> Main() { return Result::Error(7_i64); }",
    );
    let constructor =
        find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
            .into_iter()
            .next()
            .expect("Result::Error constructor");
    assert!(
        enum_constructor(input.database(), constructor).expect("declared-return constructor query").is_some(),
        "declared return context must supply the generic Result arguments"
    );
}

#[test]
fn generic_enum_constructor_uses_its_explicit_generic_call_parameter_context() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<TNext, TError> Map<TValue, TNext, TError>(Result<TValue, TError> value, TNext mapped) { return match value { Result::Ok(_) => Result::Ok(mapped), Result::Error(error) => Result::Error(error), }; } unit Main() { Result<i64, string> mapped = Map<unit, i64, string>(Result::Ok(()), 7_i64); return; }",
    );
    let constructor =
        find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
            .into_iter()
            .find(|constructor| enum_constructor(input.database(), *constructor).is_ok_and(|fact| fact.is_some()))
            .expect("Result::Ok constructor nested in the explicit generic call argument");

    assert!(
        enum_constructor(input.database(), constructor)
            .expect("generic call parameter context supplies the applied Result type")
            .is_some(),
        "the explicit Map<unit, i64, string> instantiation must contextualize Result::Ok(()) as Result<unit, string>"
    );

    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");
    let call = find_call_expression(input.database(), main).expect("Map call");
    assert!(
        call_abi_signature(input.database(), call).expect("explicit generic call signature query").is_some(),
        "the same source substitution must supply the call ABI view"
    );
    let map = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Map"))
        .expect("Map definition");
    let match_expression = find_node(input.database(), map, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("Map match expression");
    let unspecialized = enum_match(input.database(), match_expression)
        .expect_err("generic match ownership must remain unavailable before specialization");
    assert!(unspecialized.is_unavailable(), "{unspecialized:?}");
    let specialization = beskid_queries::generic_call_specialization(input.database(), call)
        .expect("Map specialization query")
        .expect("explicit Map specialization");
    assert!(
        beskid_queries::enum_match_specialization(
            input.database(),
            match_expression,
            specialization.substitutions.clone(),
        )
        .expect("generic Map match specialization")
        .is_some(),
        "the call-derived environment must materialize exact match layout and ownership facts"
    );
    for body_constructor in
        find_nodes_of_kind(input.database(), map, beskid_queries::IndexedNodeKind::EnumConstructorExpression)
    {
        assert!(
            beskid_queries::enum_constructor_specialization(
                input.database(),
                body_constructor,
                specialization.substitutions.clone(),
            )
            .expect("Map body constructor specialization query")
            .is_some(),
            "each Map body constructor must consume the same generic environment"
        );
    }
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: map, symbol: "Map".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("an enum constructor contextualized by an explicit generic call parameter lowers without HIR");
}

#[test]
fn explicit_generic_enum_with_unit_and_nominal_payloads_lowers_from_a_return() {
    let (input, isa, item) = item_fixture(
        "enum FsError { NotFound(string path) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<unit, FsError> Main(string path) { return Result<unit, FsError>::Error(FsError::NotFound(path)); }",
    );

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("a unit payload is zero-sized while the nominal error payload remains a managed pointer");
}

#[test]
fn zero_sized_unit_enum_payload_still_evaluates_its_direct_call() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Touch() { return; } Result<unit, string> Main() { return Result<unit, string>::Ok(Touch()); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let touch = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Touch"))
        .expect("Touch definition");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: touch, symbol: "Touch".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a zero-sized unit payload must preserve evaluation of its source expression");

    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main function");
    let clif = main.function.display().to_string();
    let touch = clif
        .lines()
        .find_map(|line| {
            let (function_ref, declaration) = line.trim().split_once(" = ")?;
            declaration.contains("%Touch").then_some(function_ref)
        })
        .expect("Main must import the exact Touch callee");
    assert!(
        clif.lines().any(|line| line.trim_start().starts_with("call ") && line.contains(touch)),
        "the unit payload must call the exact Touch function even though no payload bytes are stored: {clif}"
    );
}

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
    let expression = find_node(db, item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("match expression with never arm");

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
        panic!(
            "a never-returning arm terminates without supplying a merge value: {}",
            error.display_with_db(db)
        );
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
    let fiber_path = source_root.join("Concurrency/Fiber.bd");
    let fiber_error_path = source_root.join("Concurrency/FiberError.bd");
    let fiber_status_path = source_root.join("Concurrency/FiberJoinStatus.bd");
    let results_path = source_root.join("Core/Results.bd");
    let main_source = "use Concurrency.Fiber; unit Main() { Fiber<i64> fiber = Fiber<i64> { value: 7_i64, handle: -1_i64 }; Fiber<i64>.Join(fiber); return; }";
    let fiber_source = "use Concurrency.FiberError; use Concurrency.FiberJoinStatus; use Core.Results; pub type Fiber<T> { T value, i64 handle } pub Core.Results.Result<T, FiberError> Join<T>(Fiber<T> self) { FiberJoinStatus status = FiberJoinStatus::Panicked(self.handle, \"panic\"); return match status { FiberJoinStatus::Ok(_) => Result::Ok(self.value), FiberJoinStatus::Cancelled(reason, cancelerId) => Result::Error(FiberError::Cancelled(reason, cancelerId)), FiberJoinStatus::StackOverflow(limitBytes, requestedBytes) => Result::Error(FiberError::StackOverflow(limitBytes, requestedBytes)), FiberJoinStatus::Panicked(code, message) => Result::Error(FiberError::Panicked(code, message)), FiberJoinStatus::NotDone => Result::Error(FiberError::Panicked(-1_i64, \"not done\")), }; }";
    let fiber_error_source = "pub enum FiberError { Cancelled(i64 reason, i64 cancelerId), StackOverflow(i64 limitBytes, i64 requestedBytes), Panicked(i64 code, string message) }";
    let fiber_status_source = "pub enum FiberJoinStatus { Ok(i64 value), Cancelled(i64 reason, i64 cancelerId), StackOverflow(i64 limitBytes, i64 requestedBytes), Panicked(i64 code, string message), NotDone }";
    let results_source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";
    std::fs::create_dir_all(fiber_path.parent().expect("fiber parent")).expect("create source tree");
    std::fs::create_dir_all(results_path.parent().expect("results parent")).expect("create Core source tree");
    std::fs::write(&main_path, main_source).expect("write Main source");
    std::fs::write(&fiber_path, fiber_source).expect("write Fiber source");
    std::fs::write(&fiber_error_path, fiber_error_source).expect("write FiberError source");
    std::fs::write(&fiber_status_path, fiber_status_source).expect("write FiberJoinStatus source");
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
    let call = find_call_expression(input.database(), main).expect("Fiber<i64>.Join call");
    let specialization = beskid_queries::generic_call_specialization(input.database(), call)
        .expect("Join specialization query")
        .expect("the applied Fiber receiver must specialize Join<T>");
    let matched =
        find_node(input.database(), join, beskid_queries::IndexedNodeKind::MatchExpression).expect("Join status match");
    assert!(
        beskid_queries::enum_match_specialization(input.database(), matched, specialization.substitutions.clone(),)
            .expect("specialized Join match query")
            .is_some(),
        "the imported concrete FiberError must survive beside the owner-propagated T"
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
#[test]
fn parsed_test_definition_with_result_match_binding_lowers_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } test sample { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0_i64 { return; } }, Result::Error(_) => {}, }; }",
    );
    let test_item = find_test_definition(input.database(), root).expect("test item");
    let function = match emit_isle_item(&input, isa.as_ref(), test_item) {
        Ok(function) => function,
        Err(error) => {
            panic!("TestDefinition with Ok(written) match must lower: {}", error.display_with_db(input.database()))
        }
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn parsed_generic_result_match_with_nominal_error_binds_ok_payload_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum SyscallError { InvalidFd(i64 fd), IoFailure(i64 code) } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(7_i64); match result { Result::Ok(written) => { written; }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "nominal Error payload + Ok(written) binding must lower: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
}

#[test]
fn parsed_generic_result_match_arm_uses_bound_payload_in_comparison_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0_i64 { return; } }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => {
            panic!("bound payload comparison inside match arm must lower: {}", error.display_with_db(input.database()))
        }
    };
    let clif = function.display().to_string();
    assert!(clif.contains("icmp"), "{clif}");
}

#[test]
fn parsed_generic_enum_match_statement_binds_scalar_payload_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { written; }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "generic result statement match must bind Ok(written) payload: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("load.i64") || clif.contains("load"), "{clif}");
}

#[test]
fn parsed_generic_enum_match_statement_lowers_empty_unit_blocks_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, string> result = Result<i64, string>::Ok(7_i64); match result { Result::Ok(_) => {}, Result::Error(_) => {}, }; return; }",
    );
    let body = item_body(input.database(), item).expect("item body query").expect("item body");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.statement_count(body), Some(3), "function body statements");

    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "generic result statement match lowers empty unit arm blocks: {}",
            error.display_with_db(input.database())
        ),
    };

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn parsed_statement_match_lowers_empty_blocks_and_the_final_effect_in_unit_blocks() {
    let (input, isa, item) = item_fixture(
        "enum Result { Ok, Error } unit Main() { mut i64 observed = 0_i64; Result result = Result::Ok; match result { Result::Ok => { observed = 1_i64; }, Result::Error => {}, }; return; }",
    );

    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "statement-position match blocks must lower all effects and accept an empty arm: {}",
            error.display_with_db(input.database())
        ),
    };
    let clif = function.display().to_string();

    assert!(clif.contains("iconst.i64 1"), "the final arm effect must not be withheld as a value: {clif}");
}

#[test]
fn parsed_generic_enum_match_statement_lowers_direct_unit_call_arms_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Fail() { return; } unit Main() { Result<i64, i64> result = Result<i64, i64>::Error(0_i64); match result { Result::Ok(_) => {}, Result::Error(_) => Fail(), }; return; }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let fail = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Fail"))
        .expect("Fail item");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main item");
    let fail_call = find_nodes_of_kind(db, main, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|key| {
            matches!(
                call_lowering(db, *key).ok().flatten(),
                Some(beskid_queries::CallLowering::Direct(declaration)) if declaration == fail
            )
        })
        .expect("direct Fail arm call");
    assert_eq!(
        beskid_codegen::SyntaxNodeFacts::new(&input).direct_callee(fail_call),
        Some(DirectCallee::item(fail)),
        "the match arm must retain its direct unit callee"
    );

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    let imported = module.declare_function("Fail", Linkage::Import, &signature).expect("declare imported unit callee");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(DirectCallee::item(fail), imported)]));

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), main, &mut importer)
        .expect("direct unit call arm lowers through the match statement path");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "{clif}");
}

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
fn parsed_test_program_specializes_is_ok_and_binds_match_payload_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } unit True(bool condition, string because) { if condition { return; } return; } test sample { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); True(IsOk(result), \"ok\"); match result { Result::Ok(written) => { True(written >= 0_i64, \"nonneg\"); }, Result::Error(_) => {}, }; }",
    );
    let db = input.database();
    let is_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk");
    let true_fn = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("True"))
        .expect("True");
    let test = find_test_definition(db, root).expect("test");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: true_fn, symbol: "True".into() },
            SyntaxModuleItem { key: test, symbol: "sample".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("SyscallWrite-shaped IsOk + Ok(written) test must lower: {error:?}"),
    };
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("IsOk#generic_")),
        "IsOk must specialize: {:?}",
        artifact.functions.iter().map(|function| &function.name).collect::<Vec<_>>(),
    );
}

#[test]
fn parsed_test_program_specializes_a_generic_call_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } test Main { string value = \"same\"; Equal(value, value, \"because\"); }",
    );
    let generic = find_function_definition(input.database(), root).expect("generic function");
    let test = find_test_definition(input.database(), root).expect("test item");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: generic, symbol: "Equal".into() },
            SyntaxModuleItem { key: test, symbol: "Main".into() },
        ],
    )
    .expect("test-body generic calls produce exact syntax ABI specializations");

    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "test-body generic calls must emit their exact specialization",
    );
}

#[test]
fn parsed_test_program_lowers_a_bare_i64_generic_argument_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i64 Position() { return 0_i64; } unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } test Main { Equal(Position(), 0, \"initial position\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let test = find_test_definition(input.database(), root).expect("test item");
    let call = find_call_expression(input.database(), test).expect("outer Equal call");
    assert_eq!(
        call_abi_signature(input.database(), call).expect("generic call signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([
                beskid_queries::SemanticTypeId::I64,
                beskid_queries::SemanticTypeId::I64,
                beskid_queries::SemanticTypeId::STRING,
            ]),
            result: beskid_queries::SemanticTypeId::UNIT,
        }),
    );

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Position".into() },
            SyntaxModuleItem { key: items[1], symbol: "Equal".into() },
            SyntaxModuleItem { key: test, symbol: "Main".into() },
        ],
    )
    .expect("syntax lowering keeps the generic literal at the specialized ABI width");

    beskid_codegen::validate_artifact(&artifact).expect("generic artifact is ABI-valid");
    let equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Equal#generic_"))
        .expect("specialized Equal function");
    let clif = equal.function.display().to_string();
    assert!(clif.contains("i64"), "{clif}");
}

#[test]
fn cyb137_bound_payload_compare_unsuffixed_integer_must_lower() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() { Result<i64, i64> result = Result<i64, i64>::Ok(7_i64); match result { Result::Ok(written) => { if written >= 0 { return; } }, Result::Error(_) => {}, }; return; }",
    );
    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!("CYB-137 unsuffixed compare must lower: {}", error.display_with_db(input.database())),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("icmp"), "{clif}");
}

#[test]
fn cyb137_assert_true_is_ok_then_bound_payload_match_must_lower() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } enum SyscallError { InvalidFd(i64 fd) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } unit True(bool condition, string because) { if condition { return; } return; } test sample { Result<i64, SyscallError> result = Result<i64, SyscallError>::Ok(0_i64); True(IsOk(result), \"ok\"); match result { Result::Ok(written) => { True(written >= 0, \"nonneg\"); }, Result::Error(_) => {}, }; }",
    );
    let db = input.database();
    let is_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk");
    let true_fn = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("True"))
        .expect("True");
    let test = find_test_definition(db, root).expect("test");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: true_fn, symbol: "True".into() },
            SyntaxModuleItem { key: test, symbol: "sample".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("CYB-137 SyscallWrite-shaped fixture must lower: {error:?}"),
    };
    assert!(
        artifact.functions.iter().any(|f| f.name.starts_with("IsOk#generic_")),
        "IsOk specialization missing: {:?}",
        artifact.functions.iter().map(|f| &f.name).collect::<Vec<_>>(),
    );
}

#[test]
fn cyb169_enum_return_i64_main_must_lower() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result { Ok(i64 value), Error(i64 error) } Result MakeOk() { return Result::Ok(7_i64); } i64 Main() { Result result = MakeOk(); return match result { Result::Ok(value) => value, Result::Error(_) => -1_i64, }; }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let make_ok = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("MakeOk"))
        .expect("MakeOk");
    let artifact = match lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: make_ok, symbol: "MakeOk".into() },
        ],
    ) {
        Ok(artifact) => artifact,
        Err(error) => panic!("CYB-169 enum return with i64 Main must lower: {error:?}"),
    };
    assert!(artifact.functions.iter().any(|f| f.name.contains("Main")), "Main missing from artifact");
}
