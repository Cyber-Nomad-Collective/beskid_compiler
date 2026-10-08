//! Actual canonical Decoder contract and prepare-owned source proof.
#[path = "isle_adapter/support.rs"]
mod support;
use beskid_analysis::mod_host::ModSemanticAuthority;
use beskid_analysis::projects::{CompilePlan, Target, TargetKind, prepare_project_workspace};
use beskid_queries::{CompiledSerializationTarget, ModSemanticQueryAuthority, SerializationContributionKind};
use support::*;

fn fixture(output: &str) -> Result<(), String> {
    fixture_with_service_origins(output, true)
}
fn fixture_with_service_origins(output: &str, owned_origins: bool) -> Result<(), String> {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().unwrap();
    let user_root = directory.path().join("User");
    std::fs::create_dir(&user_root).unwrap();
    let path = user_root.join("Main.bd");
    let source = "use Core.Serialization.Contracts; use Core.Serialization.Errors; use Core.Serialization.Limits; use Core.Results; pub type Counter { pub i32 value, } pub type Reader { } pub unit Main() { return; }";
    std::fs::write(&path, source).unwrap();
    let manifest_path = directory.path().join("Host.bproj");
    std::fs::write(&manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"User\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
    let plan = CompilePlan {
        project_root: directory.path().into(),
        manifest_path,
        project_name: "Host".into(),
        source_root: user_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let prepared = prepare_project_workspace(&plan).unwrap();
    let generation = SyntaxGenerationId(120);
    let (base, _) = canonical_runtime_test_assembly(&mut db, directory.path(), generation);
    let mut units = base.units.as_ref().clone();
    // Runtime corpus registration does not inject the distinct Corelib service
    // corpus. Use its actual compiler-owned origins, never temporary byte copies
    // or a caller-populated trusted-path list to manufacture source authority.
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path)
            .expect("canonical service has a compiler-owned source identity");
        units.retain(|unit| unit.logical_name != canonical.logical_path);
        let (origin_path, canonical_path) = if owned_origins {
            (identity.declared_path, identity.canonical_path)
        } else {
            let copied = directory.path().join(&canonical.logical_path);
            std::fs::create_dir_all(copied.parent().unwrap()).unwrap();
            std::fs::write(&copied, &canonical.source).unwrap();
            (copied.clone(), copied)
        };
        units.push(SourceUnit {
            logical_name: canonical.logical_path,
            origin_path,
            path: canonical_path,
            program: parse_program_with_source_name("canonical-corelib", &canonical.source).unwrap(),
            source: canonical.source,
        });
    }
    units.push(SourceUnit {
        logical_name: "User.Main".into(),
        origin_path: path.clone(),
        path: path.clone(),
        source: source.into(),
        program: parse_program_with_source_name("User.Main", source).unwrap(),
    });
    let entry_index = units.len() - 1;
    let mut assembly = ProgramAssembly::new(
        base.roots.clone(),
        Arc::new(units),
        entry_index,
        base.discovery,
        base.module_index.clone(),
        false,
        generation,
    );
    assembly.verified_package_identities = prepared.verified_package_identities;
    let project = ProjectSession::new(
        &db,
        directory.path().into(),
        path.clone(),
        "serialization-target".into(),
        "canonical".into(),
    );
    let target =
        TargetMetadata::supported().into_iter().find(|t| t.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).unwrap();
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        Arc::new(assembly.clone()),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let contracts = assembly
        .units
        .iter()
        .find(|unit| unit.logical_name == beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
        .unwrap();
    let contract_key =
        AstNodeKey { unit: SourceUnitId::new(&db, contracts.path.clone()), generation, node: AstNodeId(0) };
    assert_eq!(
        beskid_queries::canonical_corelib_source_path(&db, contract_key).as_deref(),
        owned_origins.then_some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
    );
    let stale = AstNodeKey { generation: SyntaxGenerationId(119), ..contract_key };
    assert!(beskid_queries::canonical_corelib_source_path(&db, stale).is_none());
    let owner = AstNodeKey {
        unit: SourceUnitId::new(&db, path.clone()),
        generation,
        node: assembly.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).next().unwrap(),
    };
    let contribution_source = format!(
        "impl Reader : Decoder<{output}> {{ pub Result<{output},SerializationError> ReadTyped(SerializationLimits limits) {{ return Result::Error(Errors.NumericError(\"fixture\")); }} }}"
    );
    let contribution = parse_program_with_source_name("generated", &contribution_source).unwrap().node.items[0].clone();
    let carrier = {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(&db, project, &assembly).unwrap();
        use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxRequest, ModSyntaxResponse};
        let ModSyntaxResponse::Node(Some(reference)) = authority.syntax_query(991, &ModSyntaxRequest::Root).unwrap()
        else {
            panic!("caller root missing")
        };
        let routes = authority.plan_canonical_paths(
            991,
            &reference,
            &[
                "Core.Results.Result".into(),
                "Core.Serialization.Contracts.Decoder".into(),
                "Core.Serialization.Reader.Reader".into(),
                "Core.Serialization.Limits.SerializationLimits".into(),
                "Core.Serialization.Errors.NumericError".into(),
                "Core.String.Core.SortOrdinal".into(),
            ],
        );
        if owned_origins {
            let routes = routes.expect("actual owned canonical sources must provide caller-visible routes");
            assert_eq!(routes.len(), 6);
            assert_eq!(routes[0].segments, ["Core", "Results", "Result"]);
            assert_eq!(routes[1].segments, ["Core", "Serialization", "Contracts", "Decoder"]);
            assert_eq!(routes[2].segments, ["Core", "Serialization", "Reader", "Reader"]);
            // The Mod's numeric failure constructor is a function in the exact
            // canonical Errors source, routed like the Diagnostic constructor.
            assert_eq!(routes[4].logical_name, "Core.Serialization.Errors.NumericError");
            assert_eq!(routes[4].segments, ["Core", "Serialization", "Errors", "NumericError"]);
            // Ordinal key ordering is the exact Foundation String Core function.
            assert_eq!(routes[5].logical_name, "Core.String.Core.SortOrdinal");
            assert_eq!(routes[5].segments, ["Core", "String", "Core", "SortOrdinal"]);
            assert!(
                authority
                    .plan_canonical_paths(
                        991,
                        &reference,
                        &["Core.Results.Result".into(), "Core.Results.Result".into()]
                    )
                    .is_err()
            );
        } else {
            assert!(routes.is_err(), "copied canonical-looking source cannot issue a route");
        }
        let handle = authority.resolve_type(owner).unwrap();
        authority.compile_serialization_target(991, 0, handle, &contribution).unwrap()
    };
    let proof = carrier.issuer_payload::<CompiledSerializationTarget>().unwrap();
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program.node.items.push(contribution);
    let mut next = ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(units),
        assembly.entry_index,
        assembly.discovery,
        assembly.module_index.clone(),
        false,
        SyntaxGenerationId(121),
    );
    next.verified_package_identities = assembly.verified_package_identities.clone();
    next.compiled_mod_metadata.push(carrier.clone());
    let typed =
        build_typed_program_with_corelib_services(&mut db, project, next.generation, Arc::new(next), capability)
            .unwrap();
    let rebound = proof.rebind(&db, &typed).map_err(|e| e.to_string())?;
    assert_eq!(rebound.kind(), SerializationContributionKind::Decoder);
    assert_ne!(rebound.owner(), rebound.receiver(), "typed Decoder receiver is independently owned");
    assert!(rebound.shape(&db, &typed).is_ok());
    Ok(())
}
#[test]
fn canonical_decoder_target_preserves_typed_output_and_correspondence() {
    fixture("Counter").unwrap();
}
#[test]
fn canonical_decoder_cannot_bind_foreign_output() {
    assert!(fixture("Reader").is_err());
}

#[test]
fn copied_corelib_contract_bytes_cannot_bind_decoder_authority() {
    assert!(fixture_with_service_origins("Counter", false).is_err());
}

/// Bind a Decoder contribution to the type `target` and return the binding error, if any.
/// `Main.bd` is the entry unit (where the merger appends contributions) and declares `Local`
/// with a private field; `Model.bd` is a second source unit of the same package that declares
/// `Hidden` (a private field) and `Open` (only `pub` fields).
fn bind_cross_unit_target(target: &str) -> Result<(), String> {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().unwrap();
    let user_root = directory.path().join("User");
    std::fs::create_dir(&user_root).unwrap();
    let path = user_root.join("Main.bd");
    let source = "use Core.Serialization.Contracts; use Core.Serialization.Errors; use Core.Serialization.Limits; use Core.Results; pub type Local { i32 hidden, } pub type Reader { } pub unit Main() { return; }";
    std::fs::write(&path, source).unwrap();
    let model_path = user_root.join("Model.bd");
    let model_source = "pub type Hidden { pub i32 shown, i32 secret, } pub type Open { pub i32 value, }";
    std::fs::write(&model_path, model_source).unwrap();
    let manifest_path = directory.path().join("Host.bproj");
    std::fs::write(&manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"User\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
    let plan = CompilePlan {
        project_root: directory.path().into(),
        manifest_path,
        project_name: "Host".into(),
        source_root: user_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let prepared = prepare_project_workspace(&plan).unwrap();
    let generation = SyntaxGenerationId(130);
    let (base, _) = canonical_runtime_test_assembly(&mut db, directory.path(), generation);
    let mut units = base.units.as_ref().clone();
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path)
            .expect("canonical service has a compiler-owned source identity");
        units.retain(|unit| unit.logical_name != canonical.logical_path);
        units.push(SourceUnit {
            logical_name: canonical.logical_path,
            origin_path: identity.declared_path,
            path: identity.canonical_path,
            program: parse_program_with_source_name("canonical-corelib", &canonical.source).unwrap(),
            source: canonical.source,
        });
    }
    units.push(SourceUnit {
        logical_name: "User.Model".into(),
        origin_path: model_path.clone(),
        path: model_path.clone(),
        source: model_source.into(),
        program: parse_program_with_source_name("User.Model", model_source).unwrap(),
    });
    units.push(SourceUnit {
        logical_name: "User.Main".into(),
        origin_path: path.clone(),
        path: path.clone(),
        source: source.into(),
        program: parse_program_with_source_name("User.Main", source).unwrap(),
    });
    let entry_index = units.len() - 1;
    let mut assembly = ProgramAssembly::new(
        base.roots.clone(),
        Arc::new(units),
        entry_index,
        base.discovery,
        base.module_index.clone(),
        false,
        generation,
    );
    assembly.verified_package_identities = prepared.verified_package_identities;
    let project =
        ProjectSession::new(&db, directory.path().into(), path.clone(), "serialization-cross-unit".into(), "canonical".into());
    let target_metadata =
        TargetMetadata::supported().into_iter().find(|t| t.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target_metadata);
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        Arc::new(assembly.clone()),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let declaring_unit = assembly.units.iter().position(|unit| unit.path == model_path).unwrap();
    let (unit_path, index) = if target == "Local" {
        (path.clone(), assembly.entry_syntax_index())
    } else {
        (model_path.clone(), &assembly.syntax_indexes[declaring_unit])
    };
    let unit_program = &assembly.units.iter().find(|unit| unit.path == unit_path).unwrap().program;
    let node = index
        .ids_of_kind(NodeKind::TypeDefinition)
        .find(|node| {
            index
                .node_at(unit_program, *node)
                .and_then(|item| item.of::<beskid_analysis::syntax::TypeDefinition>())
                .is_some_and(|definition| definition.name.node.name == target)
        })
        .unwrap();
    let owner = AstNodeKey { unit: SourceUnitId::new(&db, unit_path), generation, node };
    let contribution_source = format!(
        "impl Reader : Decoder<{target}> {{ pub Result<{target},SerializationError> ReadTyped(SerializationLimits limits) {{ return Result::Error(Errors.NumericError(\"fixture\")); }} }}"
    );
    let contribution = parse_program_with_source_name("generated", &contribution_source).unwrap().node.items[0].clone();
    let authority = ModSemanticQueryAuthority::for_registered_assembly(&db, project, &assembly).unwrap();
    let handle = authority.resolve_type(owner).map_err(|error| error.to_string())?;
    authority.compile_serialization_target(992, 0, handle, &contribution).map(|_| ()).map_err(|error| error.to_string())
}

/// Serde precedent: a target declared in the entry unit, where the generated adapter is
/// merged, keeps its private fields readable and constructible by that adapter.
#[test]
fn entry_unit_target_with_a_private_field_binds() {
    bind_cross_unit_target("Local").unwrap();
}

/// A target in another source unit whose fields are all `pub` binds: the adapter needs no
/// private access.
#[test]
fn other_unit_target_with_only_pub_fields_binds() {
    bind_cross_unit_target("Open").unwrap();
}

/// A target in another source unit with a non-`pub` field is rejected at the binding with a
/// diagnostic that names the target, its field, and both source units. The merged adapter is
/// never attributed to the declaring unit, so ordinary entry-unit code and every other
/// contribution keep the E1211 denial.
#[test]
fn other_unit_target_with_a_private_field_is_rejected_precisely() {
    let error = bind_cross_unit_target("Hidden").expect_err("a private field of another unit cannot be bound");
    assert!(error.contains("serialization target `Hidden`"), "{error}");
    assert!(error.contains("field `secret` is not `pub`"), "{error}");
    assert!(error.contains("Model.bd"), "{error}");
    assert!(error.contains("Main.bd"), "{error}");
    assert!(error.contains("E1211"), "{error}");
    assert!(!error.contains("`shown`"), "only the private field is named: {error}");
}
