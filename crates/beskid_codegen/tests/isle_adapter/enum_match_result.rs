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
fn result_try_inside_method_body_lowers_with_its_declared_return_layout() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Closed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } unit Main() {} type Socket { pub Result<unit, Error> Complete(Result<i64, Error> value) { i64 output = value?; return Result::Ok(()); } }",
    );
    let method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("Result-returning method");
    let expression = find_node(input.database(), method, beskid_queries::IndexedNodeKind::TryExpression)
        .expect("propagation within the method body");
    let fact = beskid_queries::try_expression_fact(input.database(), expression)
        .expect("method owns its declared Result return authority")
        .expect("method-body try fact");
    assert_eq!(fact.payload_type, beskid_queries::SemanticTypeId::I64);
    assert_ne!(fact.operand_layout, fact.return_layout, "distinct success types require error re-layout");
    lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: method, symbol: "Complete".into() }])
        .expect("method-body propagation lowers through real module emission");
}

#[test]
fn nested_result_try_as_a_concrete_call_argument_lowers() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Closed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } type AddressValue { i64 value, } Result<unit, Error> Main(Socket left, Socket right) { left.Connect(right.Address()?)?; return Result::Ok(()); } type Socket { pub Result<AddressValue, Error> Address() { return Result::Ok(AddressValue { value: 1_i64 }); } pub Result<unit, Error> Connect(AddressValue address) { return Result::Ok(()); } }",
    );
    let main = super::support::named_function(&input, root, "Main");
    let expressions = find_nodes_of_kind(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression);
    assert_eq!(expressions.len(), 2);
    for expression in expressions {
        let fact = beskid_queries::try_expression_fact(input.database(), expression)
            .expect("exact nested Result authority")
            .expect("nested try fact");
        assert_eq!(beskid_queries::abi_type(input.database(), expression).unwrap(), Some(fact.payload_type));
        assert_eq!(beskid_queries::node_type(input.database(), expression).unwrap(), Some(fact.payload_type));
        assert_eq!(
            beskid_queries::managed_reference_kind(input.database(), expression).unwrap(),
            Some(if fact.payload_type == beskid_queries::SemanticTypeId::UNIT {
                beskid_queries::ManagedReferenceKind::NativeOrScalar
            } else {
                beskid_queries::ManagedReferenceKind::GcManaged
            })
        );
        let index = input.typed_program().assembly.entry_syntax_index();
        let parent = index.metadata_for(expression.generation, expression.node).unwrap().parent.unwrap();
        if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Expression) {
            let wrapper = AstNodeKey { node: parent, ..expression };
            assert_eq!(beskid_queries::abi_type(input.database(), wrapper).unwrap(), Some(fact.payload_type));
            assert_eq!(beskid_queries::node_type(input.database(), wrapper).unwrap(), Some(fact.payload_type));
            assert_eq!(
                beskid_queries::managed_reference_kind(input.database(), wrapper).unwrap(),
                beskid_queries::managed_reference_kind(input.database(), expression).unwrap()
            );
        }
    }
    let mut definitions = find_function_definitions(input.database(), root);
    definitions.extend(find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition));
    let items = definitions
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("nested try arguments lower with actual module declaration imports and allocation services");
}

#[test]
fn imported_direct_call_try_preserves_result_identity_with_distinct_success_types() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Network.Api; Result<unit, NetworkError> Main(Listener listener) { Connection accepted = listener.Accept()?; Connection connected = Api.Connect()?; return Result::Ok(()); }",
        ),
        (
            "Network/Api.bd",
            "use Core.Results; pub enum NetworkError { Closed() } pub type Connection { i64 value, } pub type Listener { pub Result<Connection, NetworkError> Accept() { return Result::Ok(Connection { value: 1_i64 }); } } pub Result<Connection, NetworkError> Connect() { return Result::Error(NetworkError::Closed); }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
    ]);
}

#[test]
fn imported_scoped_fallible_acquisition_lowers_with_existing_cleanup_conversion() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Core.Disposable; use Network.Api; Result<unit, NetworkError> Main() { use Resource resource = Api.Acquire()?; return Result::Ok(()); }",
        ),
        (
            "Network/Api.bd",
            "use Core.Results; use Core.Disposable; pub enum NetworkError { Closed() } [CleanupConversion] pub NetworkError Convert(DisposeError error) { return NetworkError::Closed; } pub type Resource: Disposable { pub Result<unit, DisposeError> Dispose() { return Result::Ok(()); } } Result<Resource, NetworkError> Fresh() { if false { return Result::Error(NetworkError::Closed); } return Result::Ok(Resource {}); } pub Result<Resource, NetworkError> Acquire() { return Fresh(); }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        (
            "Core/Disposable.bd",
            "use Core.Results; pub enum DisposeError { Failed(i64 code) } pub contract Disposable { Result<unit, DisposeError> Dispose(); }",
        ),
    ]);
}

fn assert_imported_result_lowering(sources: &[(&str, &str)]) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("try project");
    let root = directory.path().to_path_buf();
    let generation = SyntaxGenerationId(156);
    let units = sources
        .iter()
        .copied()
        .map(|(relative, source)| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("source directory");
            std::fs::write(&path, source).expect("source");
            SourceUnit {
                logical_name: relative.into(),
                origin_path: path.clone(),
                program: parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse"),
                path,
                source: source.into(),
            }
        })
        .collect::<Vec<_>>();
    let project = ProjectSession::new(&db, root.clone(), units[0].path.clone(), "Try".into(), "lock".into());
    let roots = units
        .iter()
        .map(|unit| AstNodeKey { unit: SourceUnitId::new(&db, unit.path.clone()), generation, node: AstNodeId(0) })
        .collect::<Vec<_>>();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: vec![],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed try source");
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
        .flat_map(|root| {
            let mut functions = find_function_definitions(&db, *root);
            functions.extend(find_nodes_of_kind(&db, *root, beskid_queries::IndexedNodeKind::MethodDefinition));
            functions
        })
        .map(|key| SyntaxModuleItem { key, symbol: item_name(&db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    let emitted = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("imported Result calls with distinct success types lower through real ISLE");
    for expression in find_nodes_of_kind(&db, roots[0], beskid_queries::IndexedNodeKind::TryExpression) {
        let fact = beskid_queries::try_expression_fact(&db, expression).expect("try query").expect("try fact");
        assert_eq!(fact.payload_type, beskid_queries::SemanticTypeId::POINTER);
    }
    for expression in find_nodes_of_kind(&db, roots[0], beskid_queries::IndexedNodeKind::ScopedUseStatement) {
        let fact = beskid_queries::scoped_cleanup(&db, expression).expect("cleanup query").expect("cleanup fact");
        assert!(fact.diagnostic.is_none(), "{fact:?}");
        assert!(fact.acquisition.is_some());
        assert!(fact.dispose.is_some());
        assert!(fact.conversion.is_some());
    }
    let main = emitted.functions.iter().find(|function| function.name == "Main").expect("Main");
    assert!(main.function.display().to_string().contains("brif"));
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
fn parsed_result_try_reconstructs_distinct_error_layouts_and_accepts_unit_success() {
    for (source_success, target_success, consume) in
        [("i64", "unit", "i64 value = Make()?;"), ("unit", "i64", "Make()?;")]
    {
        let source = format!(
            "enum Error {{ Failed() }} enum Result<TValue, TError> {{ Ok(TValue value), Error(TError error) }} Result<{source_success}, Error> Make() {{ return Result::Error(Error::Failed); }} Result<{target_success}, Error> Main() {{ {consume} return Result::Error(Error::Failed); }}"
        );
        let (input, isa, root) = item_fixture_with_root(&source);
        let main = super::support::named_function(&input, root, "Main");
        let expression = find_node(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression).unwrap();
        let fact = beskid_queries::try_expression_fact(input.database(), expression).unwrap().unwrap();
        let source_layout = fact.operand_layout.scalar_payload_object_layout(64, 16, 8).unwrap();
        let target_layout = fact.return_layout.scalar_payload_object_layout(64, 16, 8).unwrap();
        assert_ne!(source_layout.variants[1].payload_fields, target_layout.variants[1].payload_fields);
        let items = find_function_definitions(input.database(), root)
            .into_iter()
            .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
            .collect::<Vec<_>>();
        let emitted =
            lower_syntax_program(&input, isa.as_ref(), &items).expect("distinct Result layouts rewrap the exact error");
        let main = emitted.functions.iter().find(|function| function.name == "Main").unwrap();
        let clif = main.function.display().to_string();
        assert!(clif.contains("gc_register_root"), "managed error survives allocation: {clif}");
        assert!(input.enum_static_plan(expression).is_some(), "propagated error owns the target allocation descriptor");
    }
}

/// A `let` local is a proven try operand: its declared annotation (or its initializer identity)
/// carries the same Result application a parameter declaration would.
#[test]
fn parsed_result_try_accepts_a_declared_local_operand() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }                   Result<i64, Error> Make() { return Result::Ok(1_i64); }                   Result<unit, Error> Main() { Result<i64, Error> value = Make(); i64 output = value?; return Result::Ok(()); }";
    let (input, isa, root) = item_fixture_with_root(source);
    let main = super::support::named_function(&input, root, "Main");
    let expression = find_node(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression).unwrap();
    assert!(beskid_queries::try_expression_fact(input.database(), expression).expect("try fact").is_some());
    emit_isle_item(&input, isa.as_ref(), main).expect("declared local try operand lowers");
}

#[test]
fn parsed_result_try_rejects_unproven_operands_and_mismatched_error_identity() {
    for body in [
        "i64 output = Missing()?;",
        "i64 output = other.value?;",
        "i64 output = Wrong()?;",
    ] {
        let source = format!(
            "enum Error {{ Failed() }} enum OtherError {{ Failed() }} enum Result<TValue, TError> {{ Ok(TValue value), Error(TError error) }} type Holder {{ Result<i64, Error> value, }} Result<i64, Error> Make() {{ return Result::Ok(1_i64); }} Result<i64, OtherError> Wrong() {{ return Result::Error(OtherError::Failed); }} Result<unit, Error> Main(Holder other) {{ {body} return Result::Ok(()); }}"
        );
        let (input, isa, root) = item_fixture_with_root(&source);
        let main = super::support::named_function(&input, root, "Main");
        let expression = find_node(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression).unwrap();
        assert!(beskid_queries::try_expression_fact(input.database(), expression).is_err(), "{body}");
        assert!(emit_isle_item(&input, isa.as_ref(), main).is_err(), "{body}");
    }
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
fn direct_generic_call_match_preserves_nominal_payload_identity() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Value(i64 value) } enum Outcome<T> { Ok(T value), None } Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); } i64 Main() { Error error = Error::Value(7_i64); return match Wrap(error) { Outcome::Ok(value) => 21_i64, _ => -1_i64, }; }",
    );
    let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("direct generic call match");
    let error =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::EnumDefinition).expect("Error declaration");
    let fact = enum_match(input.database(), expression).expect("match query").expect("nominal match fact");
    assert_eq!(fact.layout.variants[0].fields[0].1, beskid_queries::AggregateFieldShape::Nominal(error));
    let beskid_queries::EnumMatchPatternFact::Enum(pattern) = &fact.arms[0].pattern else {
        panic!("Ok variant pattern");
    };
    let beskid_queries::EnumMatchPatternFact::Binding(binding) = &pattern.items[0] else {
        panic!("nominal payload binding");
    };
    assert_eq!(binding.payload, beskid_queries::AggregateFieldShape::Nominal(error));
    assert_eq!(binding.managed_reference, beskid_queries::ManagedReferenceKind::GcManaged);
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("direct nominal call-result match lowers");
}

#[test]
fn direct_generic_call_match_supports_nested_nominal_payload_pattern() {
    for (wrap, scrutinee) in [
        ("Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); }", "Wrap(error)"),
        ("Outcome<Error> Wrap(Error value) { return Outcome::Ok(value); }", "Wrap(error)"),
        ("Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); }", "joined"),
    ] {
        let local = if scrutinee == "joined" { "Outcome<Error> joined = Wrap(error);" } else { "" };
        let (input, isa, root) = item_fixture_with_root(&format!(
            "enum Error {{ Value(i64 value) }} enum Outcome<T> {{ Ok(T value), None }} {wrap} i64 Main() {{ Error error = Error::Value(7_i64); {local} return match {scrutinee} {{ Outcome::Ok(Error::Value(value)) => value, _ => -1_i64, }}; }}"
        ));
        let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).unwrap();
        let fact = enum_match(input.database(), expression).unwrap().expect("nested nominal match");
        let beskid_queries::EnumMatchPatternFact::Enum(outer) = &fact.arms[0].pattern else {
            panic!("outer enum pattern");
        };
        let beskid_queries::EnumMatchPatternFact::Enum(inner) = &outer.items[0] else {
            panic!("nested nominal enum pattern");
        };
        assert!(matches!(&inner.items[0], beskid_queries::EnumMatchPatternFact::Binding(_)));
        let items = find_function_definitions(input.database(), root)
            .into_iter()
            .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
            .collect::<Vec<_>>();
        lower_syntax_program(&input, isa.as_ref(), &items).expect("nested match and controls lower");
    }
}

#[test]
fn direct_generic_call_match_distinguishes_scalar_array_and_native_pointer_ownership() {
    for (ty, expected_shape, ownership) in [
        ("i64", beskid_queries::SemanticTypeId::I64, beskid_queries::ManagedReferenceKind::NativeOrScalar),
        ("u8[]", beskid_queries::SemanticTypeId::POINTER, beskid_queries::ManagedReferenceKind::GcManaged),
        ("pointer", beskid_queries::SemanticTypeId::POINTER, beskid_queries::ManagedReferenceKind::NativeOrScalar),
    ] {
        let (input, isa, root) = item_fixture_with_root(&format!(
            "enum Outcome<T> {{ Ok(T value), None }} Outcome<T> Wrap<T>(T value) {{ return Outcome::Ok(value); }} i64 Main({ty} value) {{ return match Wrap(value) {{ Outcome::Ok(payload) => 21_i64, _ => -1_i64, }}; }}"
        ));
        let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).unwrap();
        let fact = enum_match(input.database(), expression).unwrap().expect("direct match fact");
        let beskid_queries::EnumMatchPatternFact::Enum(pattern) = &fact.arms[0].pattern else {
            panic!("Ok pattern");
        };
        let beskid_queries::EnumMatchPatternFact::Binding(binding) = &pattern.items[0] else {
            panic!("payload binding");
        };
        assert_eq!(binding.payload, beskid_queries::AggregateFieldShape::Scalar(expected_shape), "{ty}");
        assert_eq!(binding.managed_reference, ownership, "{ty}");
        let items = find_function_definitions(input.database(), root)
            .into_iter()
            .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
            .collect::<Vec<_>>();
        lower_syntax_program(&input, isa.as_ref(), &items).expect("source-proven scalar and pointer shapes lower");
    }
}

#[test]
fn cross_unit_generic_receiver_direct_match_preserves_nominal_value() {
    let compiler = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    for (payload, make, local, scrutinee, pattern) in [
        ("FiberError", "FiberError::Cancelled(73_i64, 91_i64)", "", "child.Join()", "Result::Ok(error) => 21_i64"),
        (
            "FiberError",
            "FiberError::Cancelled(73_i64, 91_i64)",
            "",
            "child.Join()",
            "Result::Ok(FiberError::Cancelled(reason, canceler)) => reason + canceler",
        ),
        (
            "FiberError",
            "FiberError::Cancelled(73_i64, 91_i64)",
            "Result<FiberError, FiberError> joined = child.Join();",
            "joined",
            "Result::Ok(error) => 21_i64",
        ),
        ("FiberError", "FiberError::Cancelled(73_i64, 91_i64)", "", "child.Join()", "Result::Error(error) => 21_i64"),
        ("i64", "7_i64", "", "child.Join()", "Result::Ok(value) => value"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("Main.bd");
        let source = format!(
            "use Concurrency.Fiber; use Concurrency.FiberError; use Core.Results; {payload} Make() {{ return {make}; }} i64 Main() {{ Fiber<{payload}> child = spawn Make(); {local} return match {scrutinee} {{ {pattern}, _ => -1_i64, }}; }}"
        );
        std::fs::write(&source_path, &source).unwrap();
        let mut units = vec![SourceUnit {
            program: parse_program_with_source_name(source_path.to_str().unwrap(), &source).unwrap(),
            origin_path: source_path.clone(),
            path: source_path,
            logical_name: "Main.bd".into(),
            source,
        }];
        for (base, relative) in [
            (&concurrency, "Concurrency/Fiber.bd"),
            (&concurrency, "Concurrency/FiberError.bd"),
            (&foundation, "Core/Disposable.bd"),
            (&foundation, "Core/Results/Results.bd"),
        ] {
            let path = base.join(relative);
            let source = std::fs::read_to_string(&path).unwrap();
            units.push(SourceUnit {
                program: parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap(),
                origin_path: path.clone(),
                path,
                logical_name: relative.into(),
                source,
            });
        }
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory.path().to_path_buf() },
                dependencies: vec![
                    RootEntry { dependency_name: Some("concurrency".into()), source_root: concurrency.clone() },
                    RootEntry { dependency_name: Some("foundation".into()), source_root: foundation.clone() },
                ],
            },
            Arc::new(units),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            SyntaxGenerationId(151),
        ));
        let target = TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .unwrap();
        let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
        beskid_codegen::lower_syntax_assembly_entrypoint(
            &mut BeskidDatabase::default(),
            assembly,
            "Main",
            target,
            isa.as_ref(),
        )
        .expect("real Fiber.Join direct match and controls lower");
    }
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
fn imported_generic_result_match_specialization_preserves_payload_provenance() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; T RequireHttp<T>(Result<T, HttpError> value, T fallback) { return match value { Result::Ok(result) => result, Result::Error(_) => fallback, }; } bool IsError<T>(Result<T, HttpError> result, HttpError expected) { return match result { Result::Ok(_) => false, Result::Error(value) => match value { HttpError::InvalidFraming => match expected { HttpError::InvalidFraming => true, HttpError::Closed => false, }, HttpError::Closed => match expected { HttpError::InvalidFraming => false, HttpError::Closed => true, }, }, }; } bool Main(Result<Request, HttpError> value, Request fallback, HttpError expected) { Request request = RequireHttp<Request>(value, fallback); if request.id == 0_i64 { return false; } return IsError<Request>(value, expected); }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Request { i64 id, }"),
    ]);
}

#[test]
fn imported_result_binding_array_field_flows_into_a_call_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; Result<i64, HttpError> Framing(Header[] headers) { return Result::Ok(0_i64); } bool Main(Result<Request, HttpError> head) { return match head { Result::Error(_) => false, Result::Ok(value) => { Result<i64, HttpError> framing = Framing(value.headers); return match framing { Result::Ok(_) => true, Result::Error(_) => false, }; }, }; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Header { string name, } pub type Request { string method, Header[] headers, }"),
    ]);
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
fn generic_unit_arguments_preserve_effects_without_fabricating_abi_storage() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Effect() { return; } i64 Consume<T>(T value, i64 count) { return count; } i64 Main() { return Consume<unit>(Effect(), 42_i64); }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem { symbol: item_name(input.database(), key).unwrap().unwrap().to_string(), key })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("zero-sized generic parameter erasure");
    let consume = artifact.functions.iter().find(|function| function.name.starts_with("Consume")).unwrap();
    assert_eq!(consume.function.signature.params.len(), 1);
    let main = artifact.functions.iter().find(|function| function.name == "Main").unwrap();
    assert!(main.function.display().to_string().contains("Effect"));
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
        origin_path: path.clone(),
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
            origin_path: path.clone(),
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
            origin_path: path.clone(),
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

#[test]
fn imported_call_scrutinee_binding_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.method, \"POST\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { string name, } pub type Request { pub string method, pub Header[] headers, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_local_scrutinee_binding_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { Result<Request, HttpError> parsed = Parse(1_i64); match parsed { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.method, \"POST\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { string name, } pub type Request { pub string method, pub Header[] headers, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_call_scrutinee_binding_indexed_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.headers[0].name, \"host\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_call_scrutinee_binding_indexed_bytes_infer_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { match Parse(1_i64) { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.body[4], 101_u8); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_local_scrutinee_binding_indexed_field_infers_a_generic_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; use Testing.Assert; Result<Request, HttpError> Parse(i64 x) { return Result::Error(HttpError::Closed()); } unit Main() { Result<Request, HttpError> parsed = Parse(1_i64); match parsed { Result::Error(_) => Assert.Same(1_i64, 2_i64), Result::Ok(request) => { Assert.Same(request.headers[0].name, \"host\"); }, }; return; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        (
            "Http/Requests.bd",
            "pub type Header { pub string name, } pub type Request { pub string method, pub Header[] headers, pub u8[] body, }",
        ),
        ("Testing/Assert.bd", "pub unit Same<T>(T actual, T expected) { return; }"),
    ]);
}

#[test]
fn imported_result_binding_array_field_owns_a_canonical_append() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Core.Collections.Array; use Http.Errors; use Http.Requests; bool Main(Result<Request, HttpError> head, Header extra) { return match head { Result::Error(_) => false, Result::Ok(request) => { Array.Append<Header>(request.headers, extra); return true; }, }; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Core/Collections/Array.bd", "pub T[] Append<T>(mut T[] values, T value) { return values; }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Header { string name, } pub type Request { string method, Header[] headers, }"),
    ]);
}
