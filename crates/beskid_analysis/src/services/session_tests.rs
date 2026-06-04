//! Entry session registry and semantic snapshot tests.

use std::path::Path;

use crate::projects::{CompilePlan, Target, TargetKind};
use crate::services::entry_session::{
    get_or_insert_assembly, invalidate_all, invalidate_project, update_semantic_snapshot,
};
use crate::services::session::{
    SemanticSnapshot, SessionFingerprint, cached_semantic_snapshot, cached_compilation_session,
};
use crate::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
};

fn test_plan(entry: &str, lock_bytes: Option<&[u8]>) -> (CompilePlan, SessionFingerprint) {
    let root = std::env::temp_dir().join("beskid_session_test");
    let _ = std::fs::create_dir_all(&root);
    if let Some(bytes) = lock_bytes {
        std::fs::write(root.join("Project.lock"), bytes).expect("lock");
    }
    let plan = CompilePlan {
        project_root: root.clone(),
        manifest_path: root.join("Project.proj"),
        project_name: "App".to_string(),
        source_root: root.join("src"),
        target: Target {
            name: "main".to_string(),
            kind: TargetKind::App,
            entry: entry.to_string(),
        },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let fp = SessionFingerprint::for_entry(&plan, Path::new(entry));
    (plan, fp)
}

fn empty_assembly(plan: &CompilePlan) -> ProgramAssembly {
    ProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: plan.source_root.clone(),
            },
            dependencies: Vec::new(),
        },
        units: std::sync::Arc::new(Vec::new()),
        hir_units: std::sync::Arc::new(Vec::new()),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: std::sync::Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    }
}

#[test]
fn store_and_retrieve_semantic_snapshot() {
    invalidate_all();
    let (plan, fp) = test_plan("Main.bd", None);
    get_or_insert_assembly(fp.clone(), empty_assembly(&plan));
    let snap = SemanticSnapshot::from_diagnostics(&[], 1, "semantic");
    update_semantic_snapshot(&fp, snap.clone());
    let loaded = cached_semantic_snapshot(&fp).expect("snapshot");
    assert_eq!(loaded.staged_through, "semantic");
    assert_eq!(loaded.syntax_generation_id, 1);
    assert!(cached_compilation_session(&fp).is_some());
}

#[test]
fn registry_invalidates_on_lockfile_change() {
    invalidate_all();
    let (plan_a, fp_a) = test_plan("Main.bd", Some(b"lock-a"));
    get_or_insert_assembly(fp_a.clone(), empty_assembly(&plan_a));
    update_semantic_snapshot(
        &fp_a,
        SemanticSnapshot::from_diagnostics(&[], 1, "semantic"),
    );
    assert!(cached_semantic_snapshot(&fp_a).is_some());

    let plan_b = CompilePlan {
        project_root: plan_a.project_root.clone(),
        manifest_path: plan_a.manifest_path.clone(),
        project_name: plan_a.project_name.clone(),
        source_root: plan_a.source_root.clone(),
        target: plan_a.target.clone(),
        dependency_projects: plan_a.dependency_projects.clone(),
        unresolved_dependencies: plan_a.unresolved_dependencies.clone(),
        has_std_dependency: plan_a.has_std_dependency,
    };
    std::fs::write(
        plan_a.project_root.join("Project.lock"),
        b"lock-b",
    )
    .expect("rewrite lock");
    let fp_b = SessionFingerprint::for_entry(&plan_b, Path::new("Main.bd"));
    assert_ne!(fp_a, fp_b);
    assert!(cached_semantic_snapshot(&fp_b).is_none());
    assert!(cached_semantic_snapshot(&fp_a).is_some());

    invalidate_project(&plan_a.project_root);
    assert!(cached_semantic_snapshot(&fp_a).is_none());
}
