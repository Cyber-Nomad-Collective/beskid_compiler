//! Enum constructor layouts, payload slots, and contextual generic constructors.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, call_abi_signature,
    emit_isle_expression, emit_isle_item, enum_constructor, enum_match, find_call_expression, find_function_definition,
    find_function_definitions, find_node, find_nodes_of_kind, isa, item_fixture, item_fixture_with_root, item_name,
    lower_syntax_program, parse_program_with_source_name, settings,
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
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            origin_path: source_path.clone(),
            path: source_path,
            source: source.into(),
            program,
        }]),
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
    assert!(clif.contains("gc_register_root"));
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
    assert!(clif.contains("gc_register_root"), "{clif}");
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
    assert!(clif.contains("gc_register_root"));
    assert!(clif.contains("iconst.i32 0"));
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
fn ordinary_call_contextualizes_inline_generic_enum_arguments() {
    assert_ordinary_enum_argument_context(
        "use Core.Optional.Option; use Network.Types; pub i64 Resolve(Option<AddressFamily> family) { return match family { Option::Some(_) => 4_i64, Option::None => 0_i64, }; }",
        "use Core.Optional.Option; use Network.Types; use Network.Dns; i64 Main() { return Dns.Resolve(Option::Some(AddressFamily::V4)) + Dns.Resolve(Option::None); }",
        true,
    );
}

#[test]
fn ordinary_call_enum_context_rejects_unproven_parameter_identity() {
    let ordinary =
        "use Core.Optional.Option; use Network.Types; pub i64 Resolve(Option<AddressFamily> family) { return 0_i64; }";
    for (dns, main) in [
        (
            ordinary,
            "use Core.Optional.Option; use Network.Types; use Network.Dns; i64 Main() { return Dns.Unknown(Option::None); }",
        ),
        (
            ordinary,
            "use Core.Optional.Option; use Network.Types; use Network.Dns; i64 Main() { return Dns.Resolve(Option::None, 0_i64); }",
        ),
        (
            "use Core.Optional.Option; pub i64 Resolve<T>(Option<T> family) { return 0_i64; }",
            "use Core.Optional.Option; use Network.Dns; i64 Main() { return Dns.Resolve(Option::None); }",
        ),
        (
            "use Core.Optional.Option; pub i64 Resolve(Option<Unknown> family) { return 0_i64; }",
            "use Core.Optional.Option; use Network.Dns; i64 Main() { return Dns.Resolve(Option::None); }",
        ),
        (
            ordinary,
            "use Core.Optional.Option; use Network.Dns; enum AddressFamily { Other } i64 Main() { return Dns.Resolve(Option::None); }",
        ),
        (
            "use Core.Optional.Option; use Network.Types; pub i64 Resolve(Option<AddressFamily> family) { return 0_i64; } pub i64 Resolve(Option<AddressFamily> family) { return 1_i64; }",
            "use Core.Optional.Option; use Network.Types; use Network.Dns; i64 Main() { return Dns.Resolve(Option::None); }",
        ),
        (
            "use Core.Optional.Option; use Network.Types; i64 Resolve(Option<AddressFamily> family) { return 0_i64; }",
            "use Core.Optional.Option; use Network.Types; use Network.Dns; i64 Main() { return Dns.Resolve(Option::None); }",
        ),
    ] {
        assert_ordinary_enum_argument_context(dns, main, false);
    }
}

fn assert_ordinary_enum_argument_context(dns: &str, main: &str, supported: bool) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project");
    let root = directory.path().to_path_buf();
    let generation = SyntaxGenerationId(154);
    let units = [
        ("Main.bd", main),
        ("Network/Dns.bd", dns),
        ("Network/Types.bd", "pub enum AddressFamily { V4, V6 }"),
        ("Core/Optional/Option.bd", "pub enum Option<T> { Some(T value), None }"),
    ]
    .into_iter()
    .map(|(path, source)| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("source directory");
        std::fs::write(&path, source).expect("source file");
        SourceUnit {
            logical_name: path.display().to_string(),
            program: parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse"),
            origin_path: path.clone(),
            path,
            source: source.into(),
        }
    })
    .collect::<Vec<_>>();
    let main_path = units[0].path.clone();
    let project = ProjectSession::new(&db, root.clone(), main_path.clone(), "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: vec![],
        },
        Arc::new(units.clone()),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed source");
    let roots = units
        .iter()
        .map(|unit| AstNodeKey { unit: SourceUnitId::new(&db, unit.path.clone()), generation, node: AstNodeId(0) })
        .collect::<Vec<_>>();
    let constructors = find_nodes_of_kind(&db, roots[0], beskid_queries::IndexedNodeKind::EnumConstructorExpression);
    let option_constructors = constructors
        .iter()
        .copied()
        .filter(|key| {
            beskid_queries::node_span(&db, *key)
                .ok()
                .flatten()
                .is_some_and(|span| main[span.start..span.end].starts_with("Option::"))
        })
        .collect::<Vec<_>>();
    assert!(!option_constructors.is_empty());
    if !supported {
        for key in option_constructors {
            assert!(
                enum_constructor(&db, key).is_err(),
                "unproven argument must not acquire a constructor layout: {dns} {main}"
            );
        }
        return;
    }
    for key in option_constructors {
        assert!(enum_constructor(&db, key).expect("concrete call parameter context").is_some());
    }
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap();
    let input =
        CodegenInput::new(&db, typed, roots.clone().into(), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("input");
    let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    let items = roots
        .iter()
        .flat_map(|root| find_function_definitions(&db, *root))
        .map(|key| SyntaxModuleItem { key, symbol: item_name(&db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    let emitted =
        lower_syntax_program(&input, isa.as_ref(), &items).expect("ordinary call arguments lower through real ISLE");
    assert!(emitted.functions.iter().any(|function| function.function.display().to_string().contains("call")));
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
