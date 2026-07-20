use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, canonical_runtime_intrinsic_capability,
    canonical_runtime_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_codegen::{CodegenInput, CodegenInputError};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, IndexedNodeKind, ProjectSession, SourceUnitId,
    SyntaxGenerationId, TypedProgram, build_canonical_runtime_typed_program, build_typed_program,
    child_nodes, node_kind,
};

fn input_fixture() -> (BeskidDatabase, TypedProgram, AstNodeKey, TargetMetadata) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { native_word_from_pointer(0); return 7; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("target");
    (db, typed, root, target)
}

#[test]
fn ordinary_syntax_programs_cannot_import_runtime_intrinsics() {
    let (db, typed, root, target) = input_fixture();
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("ordinary input remains valid");

    assert!(input.runtime_intrinsic_capability().is_none());
    let call = find_node(&db, root, IndexedNodeKind::CallExpression).expect("ordinary source call");
    assert!(
        input
            .runtime_intrinsic_for(call, "native_word_from_pointer")
            .is_none()
    );
}

#[test]
fn exact_canonical_assembly_carries_intrinsic_authority_to_codegen() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let source = canonical_runtime_sources().pop().expect("embedded source");
    let source_path = directory.join("Bootstrap.bd");
    std::fs::write(&source_path, &source.source).expect("write canonical source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse canonical source");
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("exact canonical assembly");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&db, source_path),
        generation,
        node: AstNodeId(0),
    };
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target, manifest)
        .expect("canonical codegen input");

    assert!(input.runtime_intrinsic_capability().is_some());
}

#[test]
fn canonical_runtime_source_can_import_manifest_owned_intrinsics() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let source = canonical_runtime_sources().pop().expect("embedded source");
    let source_path = directory.join("Bootstrap.bd");
    std::fs::write(&source_path, &source.source).expect("write canonical source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse canonical source");
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("exact canonical assembly");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&db, source_path),
        generation,
        node: AstNodeId(0),
    };
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target, manifest)
        .expect("canonical codegen input");
    let call = find_node(&db, root, IndexedNodeKind::CallExpression)
        .expect("canonical runtime intrinsic call");

    let (_, intrinsic) = input
        .runtime_intrinsic_for(call, "native_word_from_pointer")
        .expect("canonical runtime call is authorized");
    assert_eq!(intrinsic.name, "native_word_from_pointer");
}

fn linux_target() -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target")
}

fn find_node(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    target: IndexedNodeKind,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(target) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_node(db, child, target))
}

#[test]
fn sole_codegen_boundary_accepts_current_syntax_roots_and_exact_abi() {
    let (db, typed, root, target) = input_fixture();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target, manifest)
        .expect("valid codegen input");
    assert_eq!(input.roots(), &[root]);
}

#[test]
fn sole_codegen_boundary_rejects_stale_roots_and_manifest_drift() {
    let (db, typed, root, target) = input_fixture();
    let stale = AstNodeKey {
        generation: SyntaxGenerationId(0),
        ..root
    };
    assert!(matches!(
        CodegenInput::new(
            &db,
            typed.clone(),
            Arc::from([stale]),
            target.clone(),
            AbiManifestV5::canonical_runtime(target.clone()),
        ),
        Err(CodegenInputError::InvalidRoot(key)) if key == stale
    ));

    let other_target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate != &target)
        .expect("other target");
    assert!(matches!(
        CodegenInput::new(
            &db,
            typed,
            Arc::from([root]),
            target,
            AbiManifestV5::canonical_runtime(other_target),
        ),
        Err(CodegenInputError::ManifestTargetMismatch)
    ));
}
