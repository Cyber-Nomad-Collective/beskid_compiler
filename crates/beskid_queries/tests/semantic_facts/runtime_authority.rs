use super::support::{key, setup};
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_corelib_service_sources,
    canonical_corelib_syscall_service_capability, canonical_corelib_syscall_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
#[cfg(target_os = "windows")]
use beskid_analysis::syntax::{CallExpression, Expression};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type,
    build_canonical_corelib_syscall_typed_program, build_typed_program_with_corelib_services,
    build_typed_program_with_corelib_syscall_services, call_abi_signature, call_lowering, primitive_numeric_conversion,
    runtime_intrinsic, value_abi_type,
};
use std::sync::Arc;

#[cfg(target_os = "windows")]
#[test]
fn canonicalized_windows_syscall_source_keeps_exact_service_authority() {
    let logical_path = CANONICAL_CORELIB_SYSCALL_SOURCE_PATH;
    let embedded = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == logical_path)
        .expect("embedded Core.Syscall source");
    // The Foundation harness constructs units this way. On Windows this is an extended-length
    // physical path, so this test proves the public builder compares the same source identity.
    let physical_path = std::fs::canonicalize(
        canonical_corelib_service_source_path(logical_path).expect("compiler-owned Core.Syscall path"),
    )
    .expect("canonical Core.Syscall path");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-pc-windows-msvc")
        .expect("Windows target");
    let manifest = AbiManifestV5::canonical_runtime(target);

    let assert_syscall_write_lowering = |path: std::path::PathBuf, source: String, authorized: bool| {
        let mut db = BeskidDatabase::default();
        let program = parse_program(&source).expect("parse Core.Syscall source");
        let generation = SyntaxGenerationId(91);
        let index = SyntaxIndex::from_program(&program, generation);
        let source_root = path.parent().expect("Core.Syscall parent").to_path_buf();
        let project = ProjectSession::new(
            &db,
            source_root.clone(),
            path.clone(),
            "beskid-corelib".into(),
            "windows-canonical-service-identity".into(),
        );
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: logical_path.into(),
                origin_path: path.clone(),
                path: path.clone(),
                source,
                program: program.clone(),
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            generation,
        ));
        build_typed_program_with_corelib_services(
            &mut db,
            project,
            generation,
            assembly,
            canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
        )
        .expect("typed program");
        let syscall_write = index
            .ids_of_kind(NodeKind::CallExpression)
            .find(|node| {
                index.node_at(&program, *node).and_then(|node| node.of::<CallExpression>()).is_some_and(|call| {
                    matches!(
                        &call.callee.node,
                        Expression::Path(path)
                            if path.node.path.node.segments.last().is_some_and(|segment| segment.node.name.node.name == "__syscall_write")
                    )
                })
            });
        let syscall_write = syscall_write.expect("Core.Syscall __syscall_write call");
        let syscall_write = AstNodeKey { unit: SourceUnitId::new(&db, path), generation, node: syscall_write };
        if authorized {
            assert!(matches!(
                call_lowering(&db, syscall_write),
                Ok(Some(beskid_queries::CallLowering::CorelibService(service)))
                    if service.name == "__syscall_write" && service.symbol == "syscall_write"
            ));
        } else {
            assert!(
                !matches!(
                    call_lowering(&db, syscall_write),
                    Ok(Some(beskid_queries::CallLowering::CorelibService(service)))
                        if service.name == "__syscall_write"
                ),
                "the copied or altered __syscall_write call must not acquire Corelib service authority"
            );
        }
    };

    assert_syscall_write_lowering(physical_path, embedded.source.clone(), true);

    let copied_directory = tempfile::tempdir().expect("copied Core.Syscall directory");
    let copied_path = copied_directory.path().join("Syscall.bd");
    std::fs::write(&copied_path, &embedded.source).expect("write copied Core.Syscall source");
    assert_syscall_write_lowering(copied_path, embedded.source.clone(), false);

    let altered_directory = tempfile::tempdir().expect("altered Core.Syscall directory");
    let altered_path = altered_directory.path().join("Syscall.bd");
    let altered_source = format!("{}\n// altered source must not inherit compiler authority\n", embedded.source);
    std::fs::write(&altered_path, &altered_source).expect("write altered Core.Syscall source");
    assert_syscall_write_lowering(altered_path, altered_source, false);
}

#[test]
fn typed_value_service_preserves_source_result_and_rejects_native_pointer_payloads() {
    let logical = "Concurrency/Fiber.bd";
    let source = canonical_corelib_service_sources().into_iter().find(|source| source.logical_path == logical).unwrap();
    let path = canonical_corelib_service_source_path(logical).unwrap();
    let program = parse_program(&source.source).unwrap();
    let generation = SyntaxGenerationId(76);
    let index = SyntaxIndex::from_program(&program, generation);
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target);
    let mut db = BeskidDatabase::default();
    let source_root = path.ancestors().nth(2).unwrap().to_path_buf();
    let project = ProjectSession::new(&db, source_root.clone(), path.clone(), "concurrency".into(), "test".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit {
            origin_path: path.clone(),
            path: path.clone(),
            logical_name: logical.into(),
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let unit = SourceUnitId::new(&db, path);
    let call = index.ids_of_kind(NodeKind::CallExpression).map(|node| AstNodeKey { unit, generation, node })
        .find(|key| matches!(call_lowering(&db, *key), Ok(Some(beskid_queries::CallLowering::CorelibService(service))) if service.name == "__fiber_join_value")).unwrap();
    let declaration = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    for source_type in [SemanticTypeId::I64, SemanticTypeId::STRING, SemanticTypeId::POINTER] {
        let enclosing = beskid_queries::GenericSpecializationInstance {
            declaration,
            declaration_identity: "Concurrency.Fiber.Join".into(),
            signature: beskid_queries::ItemSignature { parameters: Arc::from([]), result: source_type },
            substitutions: Arc::from([beskid_queries::GenericSubstitution::inferred("T", source_type)]),
            contract_witnesses: Arc::from([]),
        };
        let result = beskid_queries::specialized_corelib_value_service_result(&db, call, &enclosing);
        if source_type == SemanticTypeId::POINTER {
            assert!(result.unwrap_err().to_string().contains("proven managed source identity"));
        } else {
            assert_eq!(result.unwrap().unwrap().argument, source_type);
        }
        assert_eq!(
            call_abi_signature(&db, call).unwrap().unwrap().result,
            SemanticTypeId::U8,
            "transport ABI remains status-only"
        );
    }
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
        Some(beskid_queries::CallLowering::ManifestBuiltin(beskid_queries::ManifestBuiltin {
            name: "__str_len",
            symbol: "str_len",
        }))
    );
    assert_eq!(
        call_abi_signature(&db, call).expect("dispatch builtin ABI signature").map(|signature| signature.result),
        Some(SemanticTypeId::WORD)
    );
    assert_eq!(abi_type(&db, call), Ok(Some(SemanticTypeId::WORD)));
}

#[test]
fn dynamic_string_length_builtin_proves_explicit_i64_return_conversion() {
    let source = "i64 Main(string text) { return i64(__str_len(text)); }";
    let (db, _project, unit, generation, index) = setup(source);
    let calls = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .collect::<Vec<_>>();
    let builtin = calls
        .iter()
        .copied()
        .find(|call| matches!(call_lowering(&db, *call), Ok(Some(beskid_queries::CallLowering::ManifestBuiltin(_)))))
        .expect("manifest __str_len call");
    let conversion = calls
        .iter()
        .copied()
        .find(|call| matches!(primitive_numeric_conversion(&db, *call), Ok(Some(_))))
        .expect("word-to-i64 conversion");
    let returned = key(unit, generation, &index, NodeKind::ReturnStatement, 0);

    assert_eq!(
        call_abi_signature(&db, builtin).expect("builtin ABI signature").map(|signature| signature.result),
        Some(SemanticTypeId::WORD)
    );
    assert_eq!(value_abi_type(&db, builtin).expect("builtin value ABI"), Some(SemanticTypeId::WORD));
    assert_eq!(
        primitive_numeric_conversion(&db, conversion).expect("conversion ABI fact"),
        Some(beskid_queries::PrimitiveNumericConversion { from: SemanticTypeId::WORD, to: SemanticTypeId::I64 })
    );
    assert_eq!(value_abi_type(&db, returned).expect("return ABI fact"), Some(SemanticTypeId::I64));
}

#[test]
fn stale_legacy_builtin_shape_cannot_acquire_manifest_dispatch_authority() {
    let source = "unit Main(ptr bytes) { __bytes_set(bytes, 0, 1); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(call_lowering(&db, call).expect("legacy builtin lowering"), Some(beskid_queries::CallLowering::Dynamic));
    assert_eq!(call_abi_signature(&db, call), Err(beskid_queries::SemanticError::unavailable("call_abi_signature")));
    assert_eq!(abi_type(&db, call), Err(beskid_queries::SemanticError::unavailable("abi_type")));
}

#[test]
fn canonical_concurrency_facade_gets_service_authority_but_copied_source_does_not() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CHANNEL_SOURCE_PATH)
        .expect("embedded concurrency Channel source");
    let canonical_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH)
        .expect("canonical concurrency Channel path");
    let program = parse_program(&source.source).expect("parse concurrency Channel source");
    let generation = SyntaxGenerationId(75);
    let index = SyntaxIndex::from_program(&program, generation);
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);

    let mut db = BeskidDatabase::default();
    let source_root = canonical_path.ancestors().nth(2).expect("concurrency source root").to_path_buf();
    let project = ProjectSession::new(
        &db,
        source_root.clone(),
        canonical_path.clone(),
        "beskid-concurrency".into(),
        "canonical-concurrency-source".into(),
    );
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH.into(),
            origin_path: canonical_path.clone(),
            path: canonical_path.clone(),
            source: source.source.clone(),
            program: program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("canonical concurrency facade obtains service authority");
    let unit = SourceUnitId::new(&db, canonical_path);
    let create = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .find(|key| {
            matches!(
                call_lowering(&db, *key),
                Ok(Some(beskid_queries::CallLowering::CorelibService(service)))
                    if service.name == "__channel_create"
            )
        })
        .expect("authorized Channel create call");
    assert!(matches!(
        call_lowering(&db, create).expect("Channel service lowering"),
        Some(beskid_queries::CallLowering::CorelibService(_))
    ));

    let (ordinary_db, _project, ordinary_unit, ordinary_generation, ordinary_index) =
        setup("i64 Main() { return __channel_create(0, 0); }");
    let ordinary_call = key(ordinary_unit, ordinary_generation, &ordinary_index, NodeKind::CallExpression, 0);
    assert_eq!(
        call_lowering(&ordinary_db, ordinary_call).expect("ordinary Channel spelling"),
        Some(beskid_queries::CallLowering::Dynamic),
        "application source must not acquire concurrency service authority"
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
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            origin_path: source_path.clone(),
            path: source_path.clone(),
            source: source.source.clone(),
            program,
        }]),
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
    let forged_assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: forged_directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            origin_path: forged_path.clone(),
            path: forged_path,
            source: forged_source,
            program: forged_program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
    let generation = SyntaxGenerationId(73);
    let assembly = Arc::new(ProgramAssembly::new(
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
                origin_path: syscall_path.clone(),
                path: syscall_path.clone(),
                source: source.source.clone(),
                program: syscall_program.clone(),
            },
            SourceUnit {
                logical_name: "Main.bd".into(),
                origin_path: application_path.clone(),
                path: application_path.clone(),
                source: application_source.into(),
                program: application_program.clone(),
            },
        ]),
        1,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
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
    let forged_assembly = Arc::new(ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(vec![SourceUnit {
            logical_name: "Core/Syscall/Syscall.bd".into(),
            origin_path: syscall_path.clone(),
            path: syscall_path.clone(),
            source: forged_source,
            program: forged_program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(74),
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
