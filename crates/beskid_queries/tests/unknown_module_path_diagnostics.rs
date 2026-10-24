//! An unresolved module path is reported once, with `.` paths (preview.1 smoke test: E1108 printed
//! twice, as `Core::Text` and `Core.Text`).
use beskid_analysis::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};
use beskid_analysis::services::{PrepareOptions, ResolvedInput};
use beskid_queries::{BeskidDatabase, prepare_compilation_diagnostics_isolated, prepare_compilation_diagnostics_with_db};

fn diagnostics_of(main: &str) -> Vec<Vec<beskid_analysis::analysis::SemanticDiagnostic>> {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let src = root.join("src");
    std::fs::create_dir(&src).unwrap();
    let path = src.join("Main.bd");
    std::fs::write(&path, main).unwrap();
    let plan = CompilePlan {
        project_root: root.clone(),
        manifest_path: root.join("Fixture.bproj"),
        project_name: "Fixture".into(),
        source_root: src,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly =
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap();
    let resolved = ResolvedInput {
        source_path: path,
        source: main.into(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: Some(assembly),
    };
    let mut db = BeskidDatabase::default();
    let (_, shared, _) =
        prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::without_mods(), None).unwrap();
    let (_, isolated, _) = prepare_compilation_diagnostics_isolated(&resolved, PrepareOptions::without_mods(), None).unwrap();
    vec![shared, isolated]
}

#[test]
fn an_unknown_module_path_is_one_diagnostic_with_dot_paths() {
    for main in [
        "pub i64 Main() { return Core.Text.Length(); }",
        "use Core.Output.WriteLine; pub i64 Main() { return Core.Text.Length(); }",
        "pub unit Main(Core.Text.Thing value) { return; }",
        "pub unit Main() { let f = Core.Text.Thing; return; }",
    ] {
        for diagnostics in diagnostics_of(main) {
            let unknown =
                diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1108")).collect::<Vec<_>>();
            assert_eq!(unknown.len(), 1, "one E1108 for {main}: {diagnostics:#?}");
            for diagnostic in &diagnostics {
                assert!(!diagnostic.message.contains("::"), "Beskid paths use `.`: {}", diagnostic.message);
                assert!(!diagnostic.help.as_deref().unwrap_or_default().contains("::"), "{:?}", diagnostic.help);
            }
            assert!(unknown[0].message.contains("`Core.Text`"), "{}", unknown[0].message);
        }
    }
}
