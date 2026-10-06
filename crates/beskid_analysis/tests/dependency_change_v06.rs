use beskid_analysis::projects::dependency_edit::{
    CommitDependencyChange, DependencyIntent, DependencyIntentSource, DependencyMutation, PlanDependencyChange,
};
use beskid_analysis::projects::workflow::{RefreshScope, ResolutionPolicy};

const APP: &str = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\n";

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, DependencyMutation) {
    let directory = tempfile::tempdir().unwrap();
    let app = directory.path().join("App");
    let local = directory.path().join("Local");
    std::fs::create_dir_all(app.join("Src")).unwrap();
    std::fs::create_dir_all(local.join("Src")).unwrap();
    let manifest = app.join("App.bproj");
    std::fs::write(&manifest, APP).unwrap();
    std::fs::write(
        local.join("Local.bproj"),
        "Local { name = \"Local\" version = \"0.1.0\" }\ntarget \"Local\" { kind = \"Lib\" entry = \"Local.bd\" }\n",
    )
    .unwrap();
    let mutation = DependencyMutation::Add(DependencyIntent {
        name: "Local".into(),
        source: DependencyIntentSource::Path("../Local".into()),
    });
    (directory, manifest, mutation)
}

fn policy() -> ResolutionPolicy {
    ResolutionPolicy { locked: false, offline: true, refresh: RefreshScope::None }
}

#[test]
fn dep06_plan_is_read_only_and_commit_writes_consistent_pair() {
    let (_directory, manifest, mutation) = fixture();
    let plan = PlanDependencyChange(&manifest, &mutation, &policy()).unwrap();
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), APP);
    assert!(!manifest.with_file_name("Project.lock").exists());
    assert!(!manifest.parent().unwrap().join("obj").exists());
    assert!(plan.Report().to_string().contains("Local"));
    let report = CommitDependencyChange(plan).unwrap();
    assert!(report.to_string().contains("Local"));
    let updated = std::fs::read_to_string(&manifest).unwrap();
    let parsed = beskid_analysis::projects::parse_manifest(&updated).unwrap();
    assert_eq!(parsed.dependencies[0].path.as_deref(), Some("../Local"));
    let lock = std::fs::read_to_string(manifest.with_file_name("Project.lock")).unwrap();
    let parsed_lock = beskid_analysis::projects::ProjectLockfileV2::parse_v2(&lock).unwrap();
    assert_eq!(parsed_lock.to_v2_content(), lock);
    assert!(lock.contains("Local"));
}

#[test]
fn dep06_commit_rejects_concurrent_manifest_edit() {
    let (_directory, manifest, mutation) = fixture();
    let plan = PlanDependencyChange(&manifest, &mutation, &policy()).unwrap();
    std::fs::write(&manifest, format!("{APP}// concurrent user edit\n")).unwrap();
    assert!(CommitDependencyChange(plan).is_err());
    assert!(std::fs::read_to_string(&manifest).unwrap().ends_with("// concurrent user edit\n"));
    assert!(!manifest.with_file_name("Project.lock").exists());
}

#[test]
fn dep06_locked_mutation_does_not_create_manifest_or_lock_changes() {
    let (_directory, manifest, mutation) = fixture();
    let locked = ResolutionPolicy { locked: true, ..policy() };
    assert!(PlanDependencyChange(&manifest, &mutation, &locked).is_err());
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), APP);
    assert!(!manifest.with_file_name("Project.lock").exists());
}

#[test]
fn dep06_removing_absent_dependency_is_a_read_only_noop_without_a_lock() {
    let (_directory, manifest, _) = fixture();
    let plan = PlanDependencyChange(&manifest, &DependencyMutation::Remove("Absent".into()), &policy()).unwrap();
    assert!(plan.Report().to_string().contains("unchanged"));
    CommitDependencyChange(plan).unwrap();
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), APP);
    assert!(!manifest.with_file_name("Project.lock").exists());
    assert!(!manifest.parent().unwrap().join(".beskid").exists());
    assert!(!manifest.parent().unwrap().join("obj").exists());
}

#[test]
fn dep06_refresh_rejects_a_lock_owned_by_another_project_without_writes() {
    let (_directory, manifest, mutation) = fixture();
    CommitDependencyChange(PlanDependencyChange(&manifest, &mutation, &policy()).unwrap()).unwrap();
    let lock_path = manifest.with_file_name("Project.lock");
    let foreign = std::fs::read_to_string(&lock_path).unwrap().replace("project_name=App", "project_name=Other");
    assert!(foreign.contains("project_name=Other"));
    std::fs::write(&lock_path, &foreign).unwrap();
    let original = std::fs::read(&manifest).unwrap();
    let update = DependencyMutation::Update { package: None, version: None, all: true };
    let refresh = ResolutionPolicy { refresh: RefreshScope::All, ..policy() };
    assert!(PlanDependencyChange(&manifest, &update, &refresh).is_err());
    assert_eq!(std::fs::read(&manifest).unwrap(), original);
    assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), foreign);
}

#[test]
fn dep06_path_report_describes_actual_added_and_removed_coordinate() {
    let (_directory, manifest, mutation) = fixture();
    let plan = PlanDependencyChange(&manifest, &mutation, &policy()).unwrap();
    assert!(plan.Report().to_string().contains("added Local: path ../Local"));
    let report = CommitDependencyChange(plan).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert!(json["effects"][0]["fromVersion"].is_null());
    assert!(json["effects"][0]["toVersion"].is_null());
    assert_eq!(json["effects"][0]["action"], "added");
    assert!(json["effects"][0]["fromCoordinate"].is_null());
    assert_eq!(json["effects"][0]["toCoordinate"], "path ../Local");
    let removal = DependencyMutation::Remove("Local".into());
    let plan = PlanDependencyChange(&manifest, &removal, &policy()).unwrap();
    assert!(plan.Report().to_string().contains("removed Local: path ../Local"));
    assert!(!plan.Report().to_string().contains("absent -> absent"));
}
