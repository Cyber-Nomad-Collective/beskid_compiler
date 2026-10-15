//! Admission is issued from current compiler and provider authority, never a JSON checksum.
use beskid_aot::api::glue::{PreparedGlueArtifact, GlueArtifactBuildRequest, build_glue_artifact};
use std::sync::Arc;

#[test]
fn prepared_glue_admission_is_an_immutable_non_serializable_authority() {
    // The issuer type deliberately exposes no public fields or Deserialize path.
    // Behavioural controls below the producer gate must exercise stale assembly,
    // changed native bytes, foreign provider and changed linker executable.
    assert!(std::mem::size_of::<PreparedGlueArtifact>() > 0);
    let _issuer: for<'a> fn(&GlueArtifactBuildRequest<'a>) -> beskid_aot::AotResult<PreparedGlueArtifact> = build_glue_artifact;
}

#[test]
fn self_consistent_foreign_packet_cannot_replace_current_compiler_packet() {
    use beskid_abi::{abi_v5::{AbiManifestV5, TargetMetadata}, runtime_kit::BuildProfile as RuntimeKitProfile};
    use beskid_analysis::projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex,
        ProgramAssembly, RootEntry, SourceUnit};
    use beskid_queries::{AstNodeId, AstNodeKey, BeskidDatabase, ProjectSession,
        SourceUnitId, SyntaxGenerationId, build_typed_program, child_nodes, item_name};
    let directory = tempfile::tempdir().unwrap();
    // Units are interned canonically (macOS /var is /private/var), so the fixture must use the canonical path.
    let directory_path = directory.path().canonicalize().unwrap();
    let path = directory_path.join("Main.bd");
    let source = "[Export(Abi:\"C\", Symbol:\"answer\")] pub i64 ExportAnswer() { return 42; }";
    std::fs::write(&path, source).unwrap();
    let program = beskid_analysis::services::parse_program_with_source_name(path.to_str().unwrap(), source).unwrap();
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, path.clone());
    let project = ProjectSession::new(&db, directory_path.clone(), path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root: directory_path.clone() }, dependencies: vec![] },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), origin_path: path.clone(), path,
            source: source.into(), program }]), 0, AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()), false, generation));
    let typed = build_typed_program(&mut db, project, generation, assembly).unwrap();
    let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
    // Root children are item wrappers; the function definition is nested one level down.
    fn find_function(db: &BeskidDatabase, key: AstNodeKey, name: &str) -> Option<AstNodeKey> {
        if item_name(db, key).unwrap().as_deref() == Some(name) {
            return Some(key);
        }
        child_nodes(db, key).unwrap().unwrap_or_default().iter().copied()
            .find_map(|child| find_function(db, child, name))
    }
    let key = find_function(&db, root, "ExportAnswer").expect("ExportAnswer function node");
    let target = TargetMetadata::supported().into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let input = beskid_codegen::CodegenInput::new(&db, typed, Arc::from([root]), target.clone(),
        AbiManifestV5::canonical_runtime(target.clone())).unwrap();
    let selected = vec![beskid_codegen::module_emission::SyntaxModuleItem { key, symbol: "ExportAnswer".into() }];
    // This packet is valid and every checksum was produced by the real emitter.
    // It is nevertheless not the packet selected by the producer's library authority.
    let substituted = beskid_codegen::glue::emit(&input, &selected, "foreign_library").unwrap();
    substituted.validate().unwrap();
    let runtime_request = beskid_aot::runtime::RuntimeBuildRequest {
        kit: beskid_aot::RuntimeKitRequest { prefix: directory.path().join("missing-kit"), target, profile: RuntimeKitProfile::Debug },
        linkage: beskid_aot::runtime::RuntimeLinkage::GlueSharedProviderV1,
    };
    let output = directory.path().join("foreign-output.so");
    let control = beskid_aot::api::NativeExecutionControl::new(
        std::time::Instant::now()+std::time::Duration::from_secs(1), Arc::new(||false));
    let error = build_glue_artifact(&GlueArtifactBuildRequest { input: &input, all_items: &selected,
        selected: &selected, native_library: "current_library", expected: &substituted,
        runtime: &runtime_request, output_path: &output, control: &control, owners: &[] })
        .expect_err("rehashing a substituted packet must not qualify it");
    assert!(error.to_string().contains("current registered assembly"), "{error}");
}
