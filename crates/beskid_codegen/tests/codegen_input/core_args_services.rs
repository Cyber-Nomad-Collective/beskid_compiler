use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CodegenInput, DirectCallee, EffectiveCompilationRoots, IndexedNodeKind, ModuleIndex, NodeFacts, PathBuf,
    ProgramAssembly, ProjectSession, RootEntry, SemanticTypeId, SourceUnit, SourceUnitId, SyntaxGenerationId,
    SyntaxNodeFacts, build_typed_program_with_corelib_services, call_abi_signature, call_lowering,
    canonical_corelib_service_capability, canonical_corelib_service_source_path, canonical_corelib_service_sources,
    find_node_matching, linux_target, node_span, parse_program_with_source_name,
};

fn core_args_input(
    source_path: PathBuf,
    logical_name: &str,
    source: String,
    generation: SyntaxGenerationId,
) -> (CodegenInput<'static>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let program = parse_program_with_source_name(source_path.to_str().expect("UTF-8 source path"), &source)
        .expect("parse Core.Args fixture");
    let source_root = source_path.parent().expect("source parent").to_path_buf();
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "core-args-authority".into(),
        "core-args-authority".into(),
    );
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit { logical_name: logical_name.into(), path: source_path, source, program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("build Core.Args fixture");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest).expect("generation-safe Core.Args input");
    (input, root)
}

fn assert_args_service_denied(input: &CodegenInput<'_>, root: AstNodeKey) {
    assert!(input.runtime_intrinsic_capability().is_none());
    assert!(input.corelib_service_capability().is_none());
    let call = find_node_matching(input.database(), root, IndexedNodeKind::CallExpression, |call| {
        call_is_named(input, call, "__args_count")
    })
    .expect("__args_count call");
    assert!(!matches!(
        call_lowering(input.database(), call),
        Ok(Some(beskid_queries::CallLowering::CorelibService(_)))
    ));
    assert!(!matches!(call_abi_signature(input.database(), call), Ok(Some(_))));
    assert_eq!(SyntaxNodeFacts::new(input).direct_callee(call), None);
}

#[test]
fn canonical_core_args_services_reach_codegen_input_with_exact_signatures() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded canonical Core.Args");
    let source_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("canonical physical Core.Args path");
    let (input, root) =
        core_args_input(source_path, CANONICAL_CORELIB_ARGS_SOURCE_PATH, source.source, SyntaxGenerationId(101));

    assert!(input.runtime_intrinsic_capability().is_none());
    assert!(input.corelib_service_capability().is_some());
    let facts = SyntaxNodeFacts::new(&input);
    for (name, symbol, parameters, result) in [
        ("__args_count", "args_count", Vec::new(), SemanticTypeId::I64),
        ("__args_get", "args_get", vec![SemanticTypeId::I64], SemanticTypeId::STRING),
    ] {
        let call = find_node_matching(input.database(), root, IndexedNodeKind::CallExpression, |call| {
            call_is_named(&input, call, name)
        })
        .unwrap_or_else(|| panic!("canonical Core.Args calls {name}"));
        assert!(matches!(
            call_lowering(input.database(), call).expect("Core.Args lowering"),
            Some(beskid_queries::CallLowering::CorelibService(service))
                if service.name == name && service.symbol == symbol
        ));
        assert_eq!(
            call_abi_signature(input.database(), call).expect("service signature query"),
            Some(beskid_queries::ItemSignature { parameters: parameters.into(), result }),
        );
        assert_eq!(facts.direct_callee(call), Some(DirectCallee::corelib_service(symbol)));
    }
}

#[test]
fn copied_altered_and_user_core_args_sources_are_denied() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded canonical Core.Args");

    let copied_dir = tempfile::tempdir().expect("copied Args project").keep();
    let copied_path = copied_dir.join(CANONICAL_CORELIB_ARGS_SOURCE_PATH);
    std::fs::create_dir_all(copied_path.parent().expect("copied Args parent")).expect("create copied Args parent");
    std::fs::write(&copied_path, &source.source).expect("write copied Args");
    let (copied, copied_root) = core_args_input(
        copied_path,
        CANONICAL_CORELIB_ARGS_SOURCE_PATH,
        source.source.clone(),
        SyntaxGenerationId(102),
    );
    assert_args_service_denied(&copied, copied_root);

    let canonical_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("canonical physical Core.Args path");
    let altered_source = format!("{}\n// altered bytes", source.source);
    let (altered, altered_root) =
        core_args_input(canonical_path, CANONICAL_CORELIB_ARGS_SOURCE_PATH, altered_source, SyntaxGenerationId(103));
    assert_args_service_denied(&altered, altered_root);

    let user_dir = tempfile::tempdir().expect("user Args project").keep();
    let user_path = user_dir.join("Main.bd");
    let user_source = "i64 Main() { return __args_count(); }".to_owned();
    std::fs::write(&user_path, &user_source).expect("write user source");
    let (user, user_root) = core_args_input(user_path, "Main.bd", user_source, SyntaxGenerationId(104));
    assert_args_service_denied(&user, user_root);
}

#[cfg(unix)]
#[test]
fn symlinked_core_args_source_is_denied() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("embedded canonical Core.Args");
    let canonical_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("canonical physical Core.Args path");
    let directory = tempfile::tempdir().expect("symlinked Args project").keep();
    let symlink_path = directory.join(CANONICAL_CORELIB_ARGS_SOURCE_PATH);
    std::fs::create_dir_all(symlink_path.parent().expect("symlinked Args parent"))
        .expect("create symlinked Args parent");
    std::os::unix::fs::symlink(canonical_path, &symlink_path).expect("symlink canonical Args into user project");

    let (input, root) =
        core_args_input(symlink_path, CANONICAL_CORELIB_ARGS_SOURCE_PATH, source.source, SyntaxGenerationId(105));
    assert_args_service_denied(&input, root);
}

fn call_is_named(input: &CodegenInput<'_>, call: AstNodeKey, name: &str) -> bool {
    let Ok(Some(span)) = node_span(input.database(), call) else {
        return false;
    };
    let unit_path = call.unit.path(input.database());
    input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| {
            unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone())
                == unit_path.canonicalize().unwrap_or_else(|_| unit_path.clone())
        })
        .and_then(|unit| unit.source.get(..span.start))
        .is_some_and(|prefix| {
            let identifier = prefix
                .trim_end()
                .rsplit_once(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .map_or(prefix.trim_end(), |(_, identifier)| identifier);
            identifier == name
        })
}
