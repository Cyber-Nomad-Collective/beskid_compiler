//! Entry session registry and semantic snapshot tests.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::projects::{
    AssemblyDiscovery, CompilePlan, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, Target,
    TargetKind,
};
use crate::services::entry_session::{get_or_insert_assembly, invalidate_project, update_semantic_snapshot};
use crate::services::session::{
    SemanticSnapshot, SessionFingerprint, cached_compilation_session, cached_semantic_snapshot,
};
use crate::syntax::SyntaxGenerationId;

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
static REGISTRY_TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_plan(lock_bytes: Option<&[u8]>) -> (CompilePlan, SessionFingerprint, PathBuf, PathBuf) {
    let id = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_session_test_{id}"));
    let src_root = root.join("src");
    std::fs::create_dir_all(&src_root).expect("src root");
    let entry_path = src_root.join(format!("Entry_{id}.bd"));
    std::fs::write(&entry_path, "fn main() { return; }").expect("entry source");

    let lock_path = root.join("Project.lock");
    match lock_bytes {
        Some(bytes) => std::fs::write(&lock_path, bytes).expect("lock"),
        None => {
            let _ = std::fs::remove_file(&lock_path);
        }
    }

    let plan = CompilePlan {
        project_root: root.clone(),
        manifest_path: root.join("Project.proj"),
        project_name: "App".to_string(),
        source_root: src_root,
        target: Target {
            name: "main".to_string(),
            kind: TargetKind::App,
            entry: Some(entry_path.strip_prefix(&root).expect("entry under root").to_string_lossy().into_owned()),
        },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let fp = SessionFingerprint::for_entry(&plan, &entry_path);
    (plan, fp, root, entry_path)
}

fn empty_assembly(plan: &CompilePlan) -> ProgramAssembly {
    ProgramAssembly {
        runtime_fixture: None,
        roots: EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: plan.source_root.clone() },
            dependencies: Vec::new(),
        },
        units: std::sync::Arc::new(Vec::new()),
        syntax_indexes: std::sync::Arc::new(Vec::new()),
        generation: SyntaxGenerationId(0),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: std::sync::Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths: std::sync::Arc::from([]),
    }
}

#[test]
fn store_and_retrieve_semantic_snapshot() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    let (plan, fp, root, _entry_path) = test_plan(None);
    let session = get_or_insert_assembly(fp.clone(), empty_assembly(&plan)).unwrap();
    let generation = session.assembly.generation.0;
    let snap = SemanticSnapshot::from_diagnostics(&[], generation, "semantic");
    update_semantic_snapshot(&fp, snap.clone());
    let loaded = cached_semantic_snapshot(&fp).expect("snapshot");
    assert_eq!(loaded.staged_through, "semantic");
    assert_eq!(loaded.syntax_generation_id, generation);
    assert!(cached_compilation_session(&fp).is_some());
    invalidate_project(&root);
}

#[test]
fn registry_invalidates_on_lockfile_change() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    let (plan_a, fp_a, root, entry_path) = test_plan(Some(b"lock-a"));
    let session = get_or_insert_assembly(fp_a.clone(), empty_assembly(&plan_a)).unwrap();
    update_semantic_snapshot(&fp_a, SemanticSnapshot::from_diagnostics(&[], session.assembly.generation.0, "semantic"));
    assert!(cached_semantic_snapshot(&fp_a).is_some());

    std::fs::write(root.join("Project.lock"), b"lock-b").expect("rewrite lock");
    let fp_b = SessionFingerprint::for_entry(&plan_a, &entry_path);
    assert_ne!(fp_a, fp_b);
    assert!(cached_semantic_snapshot(&fp_b).is_none());
    assert!(cached_semantic_snapshot(&fp_a).is_some());

    invalidate_project(&root);
    assert!(cached_semantic_snapshot(&fp_a).is_none());
}

#[test]
fn newer_assembly_replaces_session_and_rejects_late_semantic_snapshot() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    let (plan, fp, root, _) = test_plan(None);
    let mut original = empty_assembly(&plan);
    original.generation = SyntaxGenerationId::allocate().unwrap();
    let previous = original.generation;
    get_or_insert_assembly(fp.clone(), original.clone()).unwrap();
    update_semantic_snapshot(&fp, SemanticSnapshot::from_diagnostics(&[], previous.0, "semantic"));
    let mut edited = empty_assembly(&plan);
    edited.generation = SyntaxGenerationId::allocate().unwrap();
    let next = edited.generation;
    let session = get_or_insert_assembly(fp.clone(), edited.clone()).unwrap();
    assert_eq!(session.assembly.generation, next);
    assert!(session.semantic_snapshot.is_none());
    update_semantic_snapshot(&fp, SemanticSnapshot::from_diagnostics(&[], previous.0, "semantic"));
    assert!(cached_semantic_snapshot(&fp).is_none(), "late work must not publish over newer authority");
    assert!(get_or_insert_assembly(fp.clone(), original).is_err(), "stale jobs cannot replace current authority");
    assert!(std::sync::Arc::ptr_eq(&session, &get_or_insert_assembly(fp.clone(), edited.clone()).unwrap()));
    edited.roots.host.source_root = root.join("different-source-root");
    assert!(get_or_insert_assembly(fp.clone(), edited).is_err(), "same-generation source conflicts must fail closed");
    invalidate_project(&root);
}
