use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId,
    TypedProgram,
};
use beskid_codegen::{CodegenInput, CodegenInputError};

fn input_fixture() -> (BeskidDatabase, TypedProgram, AstNodeKey, TargetMetadata) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 7; }";
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
    db.ensure_file_text(source_path.clone(), source.into());
    db.ensure_syntax_unit(project, entry, generation)
        .expect("syntax authority");
    let assembly = Arc::new(SyntaxProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    });
    let typed = TypedProgram {
        project,
        entry,
        generation,
        assembly,
    };
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
