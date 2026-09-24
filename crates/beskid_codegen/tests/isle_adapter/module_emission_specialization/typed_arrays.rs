//! Canonical generic array operations and typed array allocation authority.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase,
    CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH, CodegenInput, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly,
    ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem, TargetMetadata,
    build_typed_program_with_corelib_services, call_abi_signature, call_lowering, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_corelib_service_sources, find_call_expression,
    find_corelib_service_call, find_function_definition, find_function_definitions, isa, item_fixture_with_root,
    item_name, lower_syntax_program, parse_program_with_source_name, settings, typed_array_allocation,
};

#[test]
fn canonical_generic_array_operations_preserve_concrete_types_through_nested_calls_and_imports() {
    let mut db = Box::new(BeskidDatabase::default());
    let application_root = tempfile::tempdir().expect("application project").keep();
    let application_path = application_root.join("Main.bd");
    let application_source = "use Core.Collections.Array; use Core.Collections.Array.ArrayIter; unit Check(bool condition, string because) { return; } i64 Main() { mut i64[] values = Array.Empty<i64>(); values = Array.Set<i64>(values, 0, 1); Check(Array.Capacity<i64>(values) >= Array.Len<i64>(values), \"capacity\"); mut ArrayIter<i64> iterator = Array.Iterate<i64>(values); return Array.Len<i64>(values); }";
    std::fs::write(&application_path, application_source).expect("write application source");

    let array_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH)
        .expect("canonical Array physical path");
    let foundation_root = array_path
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .expect("Foundation source root")
        .to_path_buf();
    let array_source = std::fs::read_to_string(&array_path).expect("canonical Array source");
    let array_iter_path = array_path.parent().expect("Array module parent").join("Array/ArrayIter.bd");
    let array_iter_source = std::fs::read_to_string(&array_iter_path).expect("canonical ArrayIter source");
    let embedded = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH)
        .expect("embedded Array source");
    assert_eq!(embedded.source.as_ref(), array_source);
    assert_eq!(
        canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH).as_deref(),
        Some(array_path.as_path())
    );
    let application_program = parse_program_with_source_name(application_path.to_str().unwrap(), application_source)
        .expect("parse application source");
    let array_program =
        parse_program_with_source_name(array_path.to_str().unwrap(), &array_source).expect("parse canonical Array");
    let array_iter_program = parse_program_with_source_name(array_iter_path.to_str().unwrap(), &array_iter_source)
        .expect("parse canonical ArrayIter");

    let application_unit = SourceUnitId::new(&*db, application_path.clone());
    let array_unit = SourceUnitId::new(&*db, array_path.clone());
    let project = ProjectSession::new(
        &*db,
        application_root.clone(),
        application_path.clone(),
        "App".into(),
        "typed-array-specialization".into(),
    );
    let generation = SyntaxGenerationId(104);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: application_root },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: foundation_root,
            }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main.bd".into(),
                origin_path: application_path.clone(),
                path: application_path,
                source: application_source.into(),
                program: application_program,
            },
            SourceUnit {
                logical_name: "Core/Collections/Array.bd".into(),
                origin_path: array_path.clone(),
                path: array_path,
                source: array_source,
                program: array_program,
            },
            SourceUnit {
                logical_name: "Core/Collections/Array/ArrayIter.bd".into(),
                origin_path: array_iter_path.clone(),
                path: array_iter_path,
                source: array_iter_source,
                program: array_iter_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib source authority"),
    )
    .expect("typed application and canonical Array source");
    let application_root = AstNodeKey { unit: application_unit, generation, node: AstNodeId(0) };
    let array_root = AstNodeKey { unit: array_unit, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([application_root]), target, manifest)
        .expect("generation-safe typed-array input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let empty = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Empty"))
        .expect("Array.Empty declaration");
    let len = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Len"))
        .expect("Array.Len declaration");
    let set = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Set"))
        .expect("Array.Set declaration");
    let iterate = find_function_definitions(input.database(), array_root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Iterate"))
        .expect("Array.Iterate declaration");
    let length_call =
        find_corelib_service_call(input.database(), len, "__array_len").expect("canonical Array.Len service call");
    assert!(matches!(
        call_lowering(input.database(), length_call).expect("Array.Len call lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__array_len" && service.symbol == "array_len"
    ));
    assert_eq!(
        call_abi_signature(input.database(), length_call).expect("Array.Len ABI signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([beskid_queries::SemanticTypeId::POINTER]),
            result: beskid_queries::SemanticTypeId::WORD,
        })
    );
    let allocation_call = find_call_expression(input.database(), empty).expect("Array.Empty allocation call");
    assert_eq!(
        typed_array_allocation(input.database(), allocation_call)
            .expect("typed allocation query")
            .expect("canonical Array allocation fact")
            .element_parameter
            .as_ref(),
        "T"
    );
    let application_items = find_function_definitions(input.database(), application_root);
    let check = application_items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Check"))
        .expect("application Check");
    let main = application_items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("application Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: empty, symbol: "Array_Empty".into() },
            SyntaxModuleItem { key: len, symbol: "Array_Len".into() },
            SyntaxModuleItem { key: set, symbol: "Array_Set".into() },
            SyntaxModuleItem { key: iterate, symbol: "Array_Iterate".into() },
            SyntaxModuleItem { key: check, symbol: "Check".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("canonical Array.Empty<i64> lowers through descriptor-backed managed allocation");

    let empty = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Array_Empty#generic_"))
        .expect("specialized Array.Empty function");
    let clif = empty.function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(clif.contains("beskid_rt_v5_array_construction_finish"), "{clif}");
    assert!(artifact.extern_imports.iter().any(|import| import.symbol == "array_len"));
    assert_eq!(artifact.array_static_plans.len(), 1, "one descriptor request for Array.Empty<i64>");
    assert_eq!(artifact.array_static_plans[0].element_type, beskid_queries::SemanticTypeId::I64);
    assert_eq!(artifact.array_static_plans[0].length, 0);
}

#[test]
fn ordinary_source_cannot_acquire_canonical_typed_array_allocation_authority() {
    let (input, _isa, root) = item_fixture_with_root("T[] Empty<T>() { return __array_new<T>(0); }");
    let empty = find_function_definition(input.database(), root).expect("ordinary generic function");
    let allocation_call = find_call_expression(input.database(), empty).expect("ordinary typed allocation spelling");

    assert_eq!(
        typed_array_allocation(input.database(), allocation_call).expect("typed allocation query"),
        None,
        "matching syntax outside compiler-owned Foundation source must fail closed"
    );
}
