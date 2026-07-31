use std::path::PathBuf;
use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH, canonical_runtime_intrinsic_capability,
    canonical_runtime_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_codegen::{CodegenInput, CodegenInputError, SyntaxNodeFacts};
use beskid_isle::{CallKind, NodeFacts};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, IndexedNodeKind, ProjectSession, SourceUnitId, SyntaxGenerationId,
    TypedProgram, build_canonical_runtime_typed_program, build_typed_program, call_lowering, child_nodes,
    item_name, node_kind,
};

fn input_fixture() -> (BeskidDatabase, TypedProgram, AstNodeKey, TargetMetadata) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { native_word_from_pointer(0); return 7; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source).expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("target");
    (db, typed, root, target)
}

/// The exact compiler-embedded canonical runtime corpus, materialized on disk under its own
/// logical paths.
///
/// `build_canonical_runtime_typed_program` mints intrinsic authority only for the byte-for-byte
/// complete embedded corpus, so every canonical test must assemble all of it. A single unit is
/// rejected as corpus drift even when it carries real embedded runtime text.
struct CanonicalRuntimeCorpus {
    /// Owns the materialized corpus so it is removed when the test drops this value.
    _tempdir: tempfile::TempDir,
    directory: PathBuf,
    units: Vec<SourceUnit>,
    entry_index: usize,
}

impl CanonicalRuntimeCorpus {
    fn materialize() -> Self {
        let tempdir = tempfile::tempdir().expect("runtime project");
        let directory = tempdir.path().to_path_buf();
        let units = canonical_runtime_sources()
            .into_iter()
            .map(|source| {
                let path = directory.join(&source.logical_path);
                std::fs::create_dir_all(path.parent().expect("canonical source directory"))
                    .expect("create canonical source directory");
                std::fs::write(&path, &source.source).expect("write canonical source");
                let program = parse_program_with_source_name(path.to_str().unwrap(), &source.source)
                    .expect("parse canonical runtime source");
                SourceUnit { logical_name: source.logical_path, path, source: source.source, program }
            })
            .collect::<Vec<_>>();
        let entry_index = units
            .iter()
            .position(|unit| unit.logical_name == CANONICAL_BOOTSTRAP_SOURCE_PATH)
            .expect("canonical corpus contains Bootstrap");
        Self { _tempdir: tempdir, directory, units, entry_index }
    }

    fn unit_path(&self, logical_name: &str) -> PathBuf {
        self.units
            .iter()
            .find(|unit| unit.logical_name == logical_name)
            .unwrap_or_else(|| panic!("canonical corpus contains {logical_name}"))
            .path
            .clone()
    }

    fn assembly(&self) -> Arc<SyntaxProgramAssembly> {
        Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: self.directory.clone() },
                dependencies: Vec::new(),
            },
            Arc::new(self.units.clone()),
            self.entry_index,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
        ))
    }
}

/// Build the authority-bearing typed program for the whole embedded runtime corpus.
fn canonical_typed_program(
    db: &mut BeskidDatabase,
    corpus: &CanonicalRuntimeCorpus,
    generation: SyntaxGenerationId,
    manifest: &AbiManifestV5,
) -> TypedProgram {
    let project = ProjectSession::new(
        &*db,
        corpus.directory.clone(),
        corpus.units[corpus.entry_index].path.clone(),
        "beskid-runtime-native".into(),
        "canonical-runtime".into(),
    );
    build_canonical_runtime_typed_program(
        db,
        project,
        generation,
        corpus.assembly(),
        canonical_runtime_intrinsic_capability(manifest).expect("compiler authority"),
    )
    .expect("exact canonical assembly")
}

/// Codegen roots covering every unit of the canonical corpus.
fn canonical_unit_roots(db: &BeskidDatabase, typed: &TypedProgram) -> Vec<AstNodeKey> {
    typed
        .assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation: typed.generation,
            node: AstNodeId(0),
        })
        .collect()
}

#[test]
fn ordinary_syntax_programs_cannot_import_runtime_intrinsics() {
    let (db, typed, root, target) = input_fixture();
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("ordinary input remains valid");

    assert!(input.runtime_intrinsic_capability().is_none());
    let call = find_node(&db, root, IndexedNodeKind::CallExpression).expect("ordinary source call");
    assert!(input.runtime_intrinsic_for(call, "native_word_from_pointer").is_none());
}

#[test]
fn exact_canonical_assembly_carries_intrinsic_authority_to_codegen() {
    let mut db = BeskidDatabase::default();
    let corpus = CanonicalRuntimeCorpus::materialize();
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = canonical_typed_program(&mut db, &corpus, SyntaxGenerationId(1), &manifest);
    let roots = canonical_unit_roots(&db, &typed);
    let input = CodegenInput::new(&db, typed, Arc::from(roots), target, manifest).expect("canonical codegen input");

    assert!(input.runtime_intrinsic_capability().is_some());
}

#[test]
fn canonical_runtime_source_can_import_manifest_owned_intrinsics() {
    let mut db = BeskidDatabase::default();
    let corpus = CanonicalRuntimeCorpus::materialize();
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = canonical_typed_program(&mut db, &corpus, SyntaxGenerationId(1), &manifest);
    let generation = typed.generation;
    let roots = canonical_unit_roots(&db, &typed);
    let input = CodegenInput::new(&db, typed, Arc::from(roots), target, manifest).expect("canonical codegen input");
    let bootstrap = AstNodeKey {
        unit: SourceUnitId::new(&db, corpus.unit_path(CANONICAL_BOOTSTRAP_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let call = find_node_matching(&db, bootstrap, IndexedNodeKind::CallExpression, |call| {
        matches!(
            beskid_queries::runtime_intrinsic_name(&db, call).ok().flatten(),
            Some(name) if name.0.as_ref() == "native_word_from_pointer"
        )
    })
    .expect("canonical runtime intrinsic call");

    let (_, intrinsic) =
        input.runtime_intrinsic_for(call, "native_word_from_pointer").expect("canonical runtime call is authorized");
    assert_eq!(intrinsic.name, "native_word_from_pointer");
}

#[test]
fn canonical_trap_intrinsic_maps_usize_to_word_and_rejects_user_packages() {
    use beskid_abi::abi_v5::AbiType;
    use beskid_queries::{item_signature, runtime_intrinsic_name};

    let mut db = BeskidDatabase::default();
    let corpus = CanonicalRuntimeCorpus::materialize();
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = canonical_typed_program(&mut db, &corpus, SyntaxGenerationId(2), &manifest);
    let generation = typed.generation;
    let roots = canonical_unit_roots(&db, &typed);
    let input = CodegenInput::new(&db, typed, Arc::from(roots), target.clone(), manifest.clone())
        .expect("canonical codegen input");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&db, corpus.unit_path(CANONICAL_BOOTSTRAP_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };

    let trap_meta = manifest
        .trusted_runtime_intrinsics
        .iter()
        .find(|intrinsic| intrinsic.name == "trap")
        .expect("manifest owns trap");
    assert_eq!(trap_meta.symbol, "beskid_rt_v5_trap");
    assert_eq!(trap_meta.params.as_slice(), &[AbiType::U8, AbiType::Pointer, AbiType::USize]);
    assert_eq!(trap_meta.result, AbiType::Void);
    assert!(trap_meta.noreturn, "ABI never result must be recorded as noreturn Void");

    let trap = find_node_matching(&db, root, IndexedNodeKind::CallExpression, |call| {
        matches!(
            runtime_intrinsic_name(&db, call).ok().flatten(),
            Some(name) if name.0.as_ref() == "trap"
        )
    })
    .expect("canonical Trap wrapper invokes trap");
    let (_, authorized) = input.runtime_intrinsic_for(trap, "trap").expect("trusted package may import trap");
    assert_eq!(authorized.symbol, "beskid_rt_v5_trap");
    assert_eq!(
        authorized.params.as_slice(),
        &[AbiType::U8, AbiType::Pointer, AbiType::USize],
        "manifest keeps pointer-width unsigned as ABI usize"
    );
    let span = beskid_queries::node_span(&db, trap).expect("trap span").expect("current trap span");
    assert!(span.end > span.start, "trap retains a source span");

    let trap_wrapper = find_node_matching(&db, root, IndexedNodeKind::FunctionDefinition, |item| {
        matches!(beskid_queries::item_name(&db, item).ok().flatten().as_deref(), Some("Trap"))
    })
    .expect("Trap export");
    assert_eq!(
        item_signature(&db, trap_wrapper).expect("Trap signature").expect("current Trap"),
        beskid_queries::ItemSignature {
            parameters: std::sync::Arc::from([
                beskid_queries::SemanticTypeId::U8,
                beskid_queries::SemanticTypeId::POINTER,
                beskid_queries::SemanticTypeId::WORD,
            ]),
            result: beskid_queries::SemanticTypeId::NEVER,
        },
        "source Trap surface maps ABI usize to word and retains never"
    );

    let (user_db, user_typed, user_root, user_target) = input_fixture();
    let user_input = CodegenInput::new(
        &user_db,
        user_typed,
        Arc::from([user_root]),
        user_target.clone(),
        AbiManifestV5::canonical_runtime(user_target),
    )
    .expect("ordinary input");
    assert!(
        user_input.runtime_intrinsic_for(user_root, "trap").is_none(),
        "user packages cannot invoke the trusted trap intrinsic"
    );
}

#[test]
fn exact_canonical_runtime_corpus_resolves_bootstrap_helpers_but_ordinary_assemblies_do_not() {
    let mut db = BeskidDatabase::default();
    let corpus = CanonicalRuntimeCorpus::materialize();
    let target = linux_target();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = canonical_typed_program(&mut db, &corpus, SyntaxGenerationId(91), &manifest);
    let generation = typed.generation;
    let roots = canonical_unit_roots(&db, &typed);
    let input =
        CodegenInput::new(&db, typed, Arc::from(roots), target, manifest).expect("canonical Scheduler codegen input");
    let scheduler_root = AstNodeKey {
        unit: SourceUnitId::new(&db, corpus.unit_path(CANONICAL_SCHEDULER_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let wrapper = find_node_matching(&db, scheduler_root, IndexedNodeKind::FunctionDefinition, |item| {
        matches!(item_name(&db, item).ok().flatten().as_deref(), Some("FiberSpawnWithCancelSlot"))
    })
    .expect("Scheduler ABI wrapper");
    let native_pointer_call = find_node_matching(&db, wrapper, IndexedNodeKind::CallExpression, |call| {
        matches!(
            call_lowering(&db, call).ok().flatten(),
            Some(beskid_queries::CallLowering::Direct(declaration))
                if matches!(item_name(&db, declaration).ok().flatten().as_deref(), Some("NativePointer"))
        )
    });
    assert!(native_pointer_call.is_some(), "canonical Scheduler reaches Bootstrap NativePointer directly");

    let sched_init = find_node_matching(&db, scheduler_root, IndexedNodeKind::FunctionDefinition, |item| {
        matches!(item_name(&db, item).ok().flatten().as_deref(), Some("SchedInit"))
    })
    .expect("canonical Scheduler SchedInit");
    let facts = SyntaxNodeFacts::new(&input);
    for intrinsic_name in ["memory_set", "pointer_add", "raw_word_store"] {
        let call = find_node_matching(&db, sched_init, IndexedNodeKind::CallExpression, |call| {
            matches!(
                beskid_queries::runtime_intrinsic_name(&db, call).ok().flatten(),
                Some(name) if name.0.as_ref() == intrinsic_name
            )
        })
        .unwrap_or_else(|| panic!("Scheduler SchedInit invokes {intrinsic_name}"));
        assert!(
            input.runtime_intrinsic_for(call, intrinsic_name).is_some(),
            "the exact canonical Scheduler corpus must authorize {intrinsic_name}",
        );
        assert_eq!(
            facts.call_kind(call),
            Some(CallKind::RuntimeIntrinsic),
            "manifest-authorized Scheduler {intrinsic_name} must never fall through to Dynamic",
        );
    }

    let ordinary = tempfile::tempdir().expect("ordinary project").keep();
    let helper_path = ordinary.join("Helper.bd");
    let main_path = ordinary.join("Main.bd");
    let helper = "pub pointer NativePointer(word value) { return value; }";
    let main = "pointer Main() { return NativePointer(0); }";
    let ordinary_assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: ordinary.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Helper".into(),
                path: helper_path.clone(),
                source: helper.into(),
                program: parse_program_with_source_name(helper_path.to_str().unwrap(), helper).expect("parse helper"),
            },
            SourceUnit {
                logical_name: "Main".into(),
                path: main_path.clone(),
                source: main.into(),
                program: parse_program_with_source_name(main_path.to_str().unwrap(), main).expect("parse main"),
            },
        ]),
        1,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let ordinary_generation = SyntaxGenerationId(92);
    let ordinary_project =
        ProjectSession::new(&db, ordinary.clone(), main_path.clone(), "App".into(), "ordinary".into());
    let ordinary_typed =
        build_typed_program(&mut db, ordinary_project, ordinary_generation, ordinary_assembly).expect("ordinary assembly");
    let ordinary_root = AstNodeKey {
        unit: SourceUnitId::new(&db, main_path),
        generation: ordinary_typed.generation,
        node: AstNodeId(0),
    };
    let ordinary_call = find_node(&db, ordinary_root, IndexedNodeKind::CallExpression).expect("ordinary call");
    assert!(
        !matches!(call_lowering(&db, ordinary_call).ok().flatten(), Some(beskid_queries::CallLowering::Direct(_))),
        "ordinary assemblies retain explicit-import-only cross-unit resolution"
    );
}

fn find_node_matching(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    target: IndexedNodeKind,
    predicate: impl Fn(AstNodeKey) -> bool + Copy,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(target) && predicate(key) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_node_matching(db, child, target, predicate))
}

fn linux_target() -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target")
}

fn find_node(db: &dyn beskid_queries::Db, key: AstNodeKey, target: IndexedNodeKind) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(target) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_node(db, child, target))
}

#[test]
fn sole_codegen_boundary_accepts_current_syntax_roots_and_exact_abi() {
    let (db, typed, root, target) = input_fixture();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target, manifest).expect("valid codegen input");
    assert_eq!(input.roots(), &[root]);
}

#[test]
fn sole_codegen_boundary_rejects_stale_roots_and_manifest_drift() {
    let (db, typed, root, target) = input_fixture();
    let stale = AstNodeKey { generation: SyntaxGenerationId(0), ..root };
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

    let other_target =
        TargetMetadata::supported().into_iter().find(|candidate| candidate != &target).expect("other target");
    assert!(matches!(
        CodegenInput::new(&db, typed, Arc::from([root]), target, AbiManifestV5::canonical_runtime(other_target),),
        Err(CodegenInputError::ManifestTargetMismatch)
    ));
}
