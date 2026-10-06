use beskid_analysis::projects::{
    ProjectWorkspacePlan, WorkspacePrepareOptions, prepare_project_workspace_plan_with_options,
};
use std::path::Path;

fn prepare(at: &Path, version: &str, body: &str) -> beskid_analysis::projects::PreparedProjectWorkspace {
    std::fs::create_dir_all(at.join("src")).unwrap();
    std::fs::write(at.join("src/Main.bd"), body).unwrap();
    let manifest = at.join("Example.bproj");
    std::fs::write(
        &manifest,
        format!("Example {{ name = \"Example\" version = \"{version}\" }}\ntarget \"Example\" {{ kind = \"Lib\" }}\n"),
    )
    .unwrap();
    prepare_project_workspace_plan_with_options(
        &ProjectWorkspacePlan {
            project_root: at.to_owned(),
            manifest_path: manifest,
            project_name: "Example".into(),
            source_root: Some(at.join("src")),
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
        },
        WorkspacePrepareOptions::default(),
        None,
    )
    .unwrap()
}

#[test]
fn verified_package_identity_is_location_independent_and_preserves_declared_version() {
    let root = tempfile::tempdir().unwrap();
    let left = prepare(&root.path().join("left"), "1.2.3", "pub type Item { pub i32 Value, }\n");
    let right = prepare(&root.path().join("right"), "1.2.3", "pub type Item { pub i32 Value, }\n");
    let a = left.package_identities().for_source(&left.materialized_source_root.join("Main.bd")).unwrap();
    let b = right.package_identities().for_source(&right.materialized_source_root.join("Main.bd")).unwrap();
    assert_eq!(a.identity(), b.identity());
    assert_eq!(a.identity().package_name(), "Example");
    assert_eq!(a.identity().version(), "1.2.3");
    assert_eq!(a.identity().source_digest().len(), 64);
    assert!(left.package_identities().for_source(&root.path().join("outside.bd")).is_none());
}

#[test]
fn package_identity_changes_for_version_or_source_and_rejects_stale_materialization() {
    let root = tempfile::tempdir().unwrap();
    let a = prepare(&root.path().join("a"), "1.0.0", "pub type Item { pub i32 Value, }\n");
    let b = prepare(&root.path().join("b"), "2.0.0", "pub type Item { pub i32 Value, }\n");
    let c = prepare(&root.path().join("c"), "1.0.0", "pub type Item { pub i64 Value, }\n");
    let get = |workspace: &beskid_analysis::projects::PreparedProjectWorkspace| {
        workspace
            .package_identities()
            .for_source(&workspace.materialized_source_root.join("Main.bd"))
            .unwrap()
            .identity()
            .clone()
    };
    assert_ne!(get(&a), get(&b));
    assert_ne!(get(&a), get(&c));
    a.package_identities().validate().unwrap();
    std::fs::write(a.materialized_source_root.join("Main.bd"), "changed").unwrap();
    assert!(a.package_identities().validate().is_err());
}

#[test]
fn verified_source_rejects_editor_override_and_ignores_private_generated_content() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepare(root.path(), "1.0.0", "pub type Item { pub i32 Value, }\n");
    let path = workspace.materialized_source_root.join("Main.bd");
    assert!(
        workspace.package_identities().validate_source(&path, "pub type Item { pub i32 Value, }\n").unwrap().is_some()
    );
    assert!(workspace.package_identities().validate_source(&path, "pub type Item { pub i64 Value, }\n").is_err());
    std::fs::create_dir(workspace.materialized_source_root.join("obj")).unwrap();
    std::fs::write(workspace.materialized_source_root.join("obj/Generated.bd"), "generated").unwrap();
    workspace.package_identities().validate().unwrap();
}

#[test]
fn nested_project_is_outside_package_while_package_tests_directory_ships() {
    let root = tempfile::tempdir().unwrap();
    let at = root.path().join("pkg");
    std::fs::create_dir_all(at.join("src/tests")).unwrap();
    std::fs::write(at.join("src/tests/Helper.bd"), "pub type Helper { pub i32 Value, }\n").unwrap();
    std::fs::create_dir_all(at.join("src/nested/src")).unwrap();
    std::fs::write(at.join("src/nested/Nested.bproj"), "Nested { name = \"Nested\" version = \"1.0.0\" }\n").unwrap();
    std::fs::write(at.join("src/nested/src/Inner.bd"), "pub type Inner { pub i32 Value, }\n").unwrap();
    let workspace = prepare(&at, "1.0.0", "pub type Item { pub i32 Value, }\n");
    let materialized = &workspace.materialized_source_root;
    assert!(materialized.join("tests/Helper.bd").is_file(), "a package tests directory is package source");
    assert!(!materialized.join("nested").exists(), "a nested project is not materialized into its parent");
    workspace.package_identities().validate().unwrap();
    assert!(workspace.package_identities().for_source(&materialized.join("tests/Helper.bd")).is_some());
    assert!(workspace.package_identities().for_source(&at.join("src/nested/src/Inner.bd")).is_none());
}

#[test]
fn rematerialization_removes_deleted_sources_and_keeps_private_build_output() {
    let root = tempfile::tempdir().unwrap();
    let at = root.path().join("pkg");
    std::fs::create_dir_all(at.join("src/old")).unwrap();
    std::fs::write(at.join("src/Old.bd"), "pub type Old { pub i32 Value, }\n").unwrap();
    std::fs::write(at.join("src/old/Gone.bd"), "pub type Gone { pub i32 Value, }\n").unwrap();
    let first = prepare(&at, "1.0.0", "pub type Item { pub i32 Value, }\n");
    let materialized = first.materialized_source_root.clone();
    assert!(materialized.join("Old.bd").is_file());
    std::fs::create_dir_all(materialized.join("obj")).unwrap();
    std::fs::write(materialized.join("obj/Generated.bd"), "generated").unwrap();
    std::fs::remove_file(at.join("src/Old.bd")).unwrap();
    std::fs::remove_dir_all(at.join("src/old")).unwrap();
    let second = prepare(&at, "1.0.0", "pub type Item { pub i32 Value, }\n");
    assert!(!materialized.join("Old.bd").exists(), "deleted source file must not linger");
    assert!(!materialized.join("old").exists(), "deleted source directory must not linger");
    assert!(materialized.join("obj/Generated.bd").is_file(), "private build output is not package source");
    second.package_identities().validate().unwrap();
}

#[test]
fn package_relative_source_path_is_slash_separated_at_the_producer() {
    // Container identity compares this producer value with `/`-separated
    // logical paths such as `Core/Collections/Map.bd` on every host, so the
    // producer emits `/` regardless of the platform separator.
    let root = tempfile::tempdir().unwrap();
    let at = root.path().join("pkg");
    std::fs::create_dir_all(at.join("src/Core/Collections")).unwrap();
    std::fs::write(at.join("src/Core/Collections/Map.bd"), "pub type Map { pub i32 Value, }\n").unwrap();
    let workspace = prepare(&at, "1.0.0", "pub type Item { pub i32 Value, }\n");
    let source = workspace.materialized_source_root.join("Core").join("Collections").join("Map.bd");
    let package = workspace.package_identities().for_source(&source).unwrap();
    let relative = package.relative_source_path(&source).unwrap();
    assert_eq!(relative, "Core/Collections/Map.bd");
    assert!(!relative.contains('\\'));
    assert_eq!(package.relative_source_path(&workspace.materialized_source_root.join("Main.bd")).as_deref(), Some("Main.bd"));
}

#[test]
fn materialized_copy_inside_a_project_root_source_validates_against_its_own_root() {
    let root = tempfile::tempdir().unwrap();
    let at = root.path().join("pkg");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("Main.bd"), "pub type Item { pub i32 Value, }\n").unwrap();
    let manifest = at.join("Example.bproj");
    std::fs::write(&manifest, "Example { name = \"Example\" version = \"1.0.0\" }\ntarget \"Example\" { kind = \"Lib\" }\n")
        .unwrap();
    let workspace = prepare_project_workspace_plan_with_options(
        &ProjectWorkspacePlan {
            project_root: at.clone(),
            manifest_path: manifest,
            project_name: "Example".into(),
            source_root: Some(at.clone()),
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
        },
        WorkspacePrepareOptions::default(),
        None,
    )
    .unwrap();
    let materialized = workspace.materialized_source_root.join("Main.bd");
    assert!(materialized.starts_with(&at), "the materialized copy lives inside the project root");
    workspace.package_identities().validate().unwrap();
    assert!(
        workspace
            .package_identities()
            .validate_source(&materialized, "pub type Item { pub i32 Value, }\n")
            .unwrap()
            .is_some()
    );
    assert!(workspace.package_identities().validate_source(&materialized, "changed").is_err());
}

#[test]
fn glue_host_at_project_root_owns_its_nominal_sources_through_the_nested_materialized_copy() {
    // The `glue/manual/owned` fixture layout: `root = "."`, so the original source root is the
    // project root and its materialized copy lives at `<project>/obj/beskid/root/<name>`.
    let root = tempfile::tempdir().unwrap();
    let at = root.path().join("manual_owned");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("ManualTypes.bd"), "pub type Pair { pub i32 Left, pub i32 Right, }\n").unwrap();
    std::fs::write(at.join("ManualExport.bd"), "pub type Unit { pub i32 Value, }\n").unwrap();
    let manifest = at.join("manual_owned.bproj");
    std::fs::write(
        &manifest,
        "manual_owned { name = \"manual_owned\" version = \"0.1.0\" root = \".\" }\ntarget \"ManualOwned\" { kind = \"Lib\" }\n",
    )
    .unwrap();
    let workspace = prepare_project_workspace_plan_with_options(
        &ProjectWorkspacePlan {
            project_root: at.clone(),
            manifest_path: manifest,
            project_name: "manual_owned".into(),
            source_root: Some(at.clone()),
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
        },
        WorkspacePrepareOptions::default(),
        None,
    )
    .unwrap();
    let identities = workspace.package_identities();
    identities.validate().unwrap();
    let materialized = workspace.materialized_source_root.join("ManualTypes.bd");
    assert!(materialized.starts_with(at.join("obj")), "the materialized copy lives inside the project root");
    for source in [&materialized, &at.join("ManualTypes.bd")] {
        let owner = identities.for_source(source).expect("the host package owns its source");
        assert_eq!(owner.identity().package_name(), "manual_owned");
        assert_eq!(owner.relative_source_path(source).as_deref(), Some("ManualTypes.bd"));
    }
    let owner = identities.for_source(&materialized).unwrap();
    assert_eq!(
        owner.relative_source_path(&workspace.materialized_source_root.join("ManualExport.bd")).as_deref(),
        Some("ManualExport.bd")
    );
    assert_eq!(owner.project_relative_source_path(&at.join("ManualTypes.bd")).as_deref(), Some("ManualTypes.bd"));
    // Private build output under the materialized root is not package source, and the shallower
    // original root never claims it as `obj/...`.
    std::fs::create_dir_all(workspace.materialized_source_root.join("obj")).unwrap();
    let generated = workspace.materialized_source_root.join("obj/Generated.bd");
    std::fs::write(&generated, "generated").unwrap();
    assert!(identities.for_source(&generated).is_none());
    assert!(owner.relative_source_path(&generated).is_none());
}
