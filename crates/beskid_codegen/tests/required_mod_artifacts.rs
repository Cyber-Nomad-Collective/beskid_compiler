//! Every discovered compiler Mod requires an executable artifact before any phase.
use beskid_analysis::mod_host::{ModHostInput, collect_mod_target_fingerprint, native_invoker_for_plan};
use beskid_analysis::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};

#[test]
fn discovered_mod_without_descriptor_cannot_be_silently_skipped() {
    let root = tempfile::tempdir().unwrap();
    let module = root.path().join("ModA");
    std::fs::create_dir_all(module.join("Src")).unwrap();
    std::fs::write(
        module.join("ModA.bproj"),
        "ModA { name = \"ModA\" version = \"0.1.0\" type = Mod mod { capabilities = [emit_syntax] } }",
    )
    .unwrap();
    let plan = CompilePlan {
        project_root: root.path().to_owned(),
        manifest_path: root.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: root.path().join("Src"),
        target: Target { name: "main".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "ModA".into(),
            manifest_path: module.join("ModA.bproj"),
            project_root: module.clone(),
            project_name: "ModA".into(),
            source_root: module.join("Src"),
        }],
        unresolved_dependencies: vec![],
        has_std_dependency: false,
    };
    assert!(native_invoker_for_plan(&plan, None).is_err(), "missing descriptor silently disabled required Mod");
    let input = ModHostInput { compile_plan: Some(&plan), ..Default::default() };
    assert!(collect_mod_target_fingerprint(&input).is_err(), "missing descriptor produced successful empty collection");
}
