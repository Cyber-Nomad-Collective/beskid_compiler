use beskid_analysis::projects::{collect_dependency_projects, parse_manifest};
use beskid_analysis::projects::graph::builder::build_project_graph_from_manifest;

const ORIGINAL: &str = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\n";

#[test]
fn dep06_planning_uses_edited_intent_without_writing_or_rebasing_paths() {
    let directory = tempfile::tempdir().unwrap();
    let app = directory.path().join("App");
    let dependency = directory.path().join("café package");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&dependency).unwrap();
    let manifest = app.join("App.bproj");
    std::fs::write(&manifest, ORIGINAL).unwrap();
    std::fs::write(dependency.join("Local.bproj"), "Local { name = \"Local\" version = \"0.1.0\" }\ntarget \"Local\" { kind = \"Lib\" entry = \"Local.bd\" }\n").unwrap();
    let edited = format!("{ORIGINAL}dependency \"Local\" {{ source = \"path\" path = \"../café package\" }}\n");
    let graph = build_project_graph_from_manifest(&manifest, parse_manifest(&edited).unwrap()).unwrap();
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), ORIGINAL);
    assert!(!app.join("Project.lock").exists());
    assert!(!app.join("obj").exists());
    assert_eq!(graph.root_project_root, app.canonicalize().unwrap());
    let dependencies = collect_dependency_projects(&graph);
    let local = dependencies.iter().filter(|entry| entry.dependency_name == "Local").collect::<Vec<_>>();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].project_root, dependency.canonicalize().unwrap());
    assert_eq!(graph.root_manifest.dependencies.len(), 1);
}

#[test]
fn dep06_proposed_graph_rejects_invalid_root_intent() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = directory.path().join("App.bproj");
    std::fs::write(&manifest, ORIGINAL).unwrap();
    let mut proposed = parse_manifest(ORIGINAL).unwrap();
    proposed.project.name.clear();
    assert!(build_project_graph_from_manifest(&manifest, proposed).is_err());
    assert_eq!(std::fs::read_to_string(manifest).unwrap(), ORIGINAL);
}
