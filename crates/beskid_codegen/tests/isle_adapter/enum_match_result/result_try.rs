//! Result `?` lowering, operand proofs, and error-layout reconstruction.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, emit_isle_item,
    find_function_definition, find_function_definitions, find_node, find_nodes_of_kind, isa, item_fixture_with_root,
    item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

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
    let main = super::super::support::named_function(&input, root, "Main");
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

pub(super) fn assert_imported_result_lowering(sources: &[(&str, &str)]) {
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
        let main = super::super::support::named_function(&input, root, "Main");
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
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }                   Result<unit, Error> Main() { Result<i64, Error> value = Result::Ok(1_i64); i64 output = value?; return Result::Ok(()); }";
    let (input, isa, root) = item_fixture_with_root(source);
    let main = super::super::support::named_function(&input, root, "Main");
    let expression = find_node(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression).unwrap();
    assert!(beskid_queries::try_expression_fact(input.database(), expression).expect("try fact").is_some());
    emit_isle_item(&input, isa.as_ref(), main).expect("declared local try operand lowers");
}

#[test]
fn parsed_result_try_rejects_unproven_operands_and_mismatched_error_identity() {
    for body in ["i64 output = Missing()?;", "i64 output = other.value?;", "i64 output = Wrong()?;"] {
        let source = format!(
            "enum Error {{ Failed() }} enum OtherError {{ Failed() }} enum Result<TValue, TError> {{ Ok(TValue value), Error(TError error) }} type Holder {{ Result<i64, Error> value, }} Result<i64, Error> Make() {{ return Result::Ok(1_i64); }} Result<i64, OtherError> Wrong() {{ return Result::Error(OtherError::Failed); }} Result<unit, Error> Main(Holder other) {{ {body} return Result::Ok(()); }}"
        );
        let (input, isa, root) = item_fixture_with_root(&source);
        let main = super::super::support::named_function(&input, root, "Main");
        let expression = find_node(input.database(), main, beskid_queries::IndexedNodeKind::TryExpression).unwrap();
        assert!(beskid_queries::try_expression_fact(input.database(), expression).is_err(), "{body}");
        assert!(emit_isle_item(&input, isa.as_ref(), main).is_err(), "{body}");
    }
}
