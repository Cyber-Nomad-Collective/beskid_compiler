use super::support::{key, setup};
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, canonical_corelib_service_source_path,
    canonical_corelib_syscall_service_capability, canonical_corelib_syscall_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_canonical_corelib_syscall_typed_program, build_typed_program_with_corelib_syscall_services, call_lowering,
    runtime_intrinsic,
};
use std::sync::Arc;

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
        Some(beskid_queries::CallLowering::Dynamic)
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
                path: syscall_path.clone(),
                source: source.source.clone(),
                program: syscall_program.clone(),
            },
            SourceUnit {
                logical_name: "Main.bd".into(),
                path: application_path.clone(),
                source: application_source.into(),
                program: application_program.clone(),
            },
        ]),
        1,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(0),
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let generation = SyntaxGenerationId(73);
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
            path: syscall_path.clone(),
            source: forged_source,
            program: forged_program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
