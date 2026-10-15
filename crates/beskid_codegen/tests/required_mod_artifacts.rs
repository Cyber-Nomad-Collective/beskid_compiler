//! Every discovered compiler Mod requires a native Mod host and an executable artifact before any
//! phase.
use beskid_analysis::mod_host::{ModHost, ModHostInput, collect_mod_target_fingerprint};
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
        has_core_dependency: false,
    };
    let input = ModHostInput {
        semantic_scope: None,
        semantic_authority: None,
        syntax_generation_id: None,
        compile_plan: Some(&plan),
        source_name: "Main.bd",
        source: "",
        pipeline: None,
        mod_host: &ModHost::NoMods,
        cached_target_fingerprint: None,
    };
    let error = collect_mod_target_fingerprint(&input)
        .err()
        .expect("a required Mod without a Mod host produced a successful empty collection")
        .to_string();
    assert!(error.contains("`ModA`") && error.contains("ModHost::native"), "{error}");
}
