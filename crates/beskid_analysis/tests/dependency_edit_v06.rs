use std::path::PathBuf;

use beskid_analysis::projects::dependency_edit::{
    DependencyIntent, DependencyIntentSource, DependencyMutation, EditManifest, SelectDependencyProject,
};

const MANIFEST: &str = "// café: keep this comment\r\nApp {\r\n  name = \"App\"\r\n  version = \"0.1.0\"\r\n}\r\n\r\ntarget \"App\" {\r\n  kind = \"App\"\r\n  entry = \"Main.bd\"\r\n}\r\n";

fn registry(name: &str, version: &str) -> DependencyMutation {
    DependencyMutation::Add(DependencyIntent {
        name: name.into(),
        source: DependencyIntentSource::Registry { registry: None, version: version.into() },
    })
}

#[test]
fn dep06_preserves_bsol_bytes_and_crlf_when_adding() {
    let updated = EditManifest(MANIFEST, &registry("Numbers", "1.2.3")).unwrap();
    assert!(updated.starts_with(MANIFEST));
    assert!(!updated.replace("\r\n", "").contains('\n'));
    let parsed = beskid_analysis::projects::parse_manifest(&updated).unwrap();
    assert_eq!(parsed.dependencies.len(), 1);
    assert_eq!(parsed.dependencies[0].name, "Numbers");
    assert_eq!(parsed.dependencies[0].version.as_deref(), Some("1.2.3"));
    assert_eq!(EditManifest(&updated, &registry("Numbers", "1.2.3")).unwrap(), updated);
    assert!(EditManifest(&updated, &registry("Numbers", "1.2.4")).is_err());
}

#[test]
fn dep06_remove_preserves_surrounding_comments_and_shared_intent() {
    let original = format!(
        "{MANIFEST}// dependency note\r\ndependency \"Numbers\" {{\r\n  source = \"registry\"\r\n  version = \"1.2.3\"\r\n}} // trailing comment\r\n\r\ndependency \"Other\" {{\r\n  source = \"path\"\r\n  path = \"../with spaces\"\r\n}}\r\n"
    );
    let updated = EditManifest(&original, &DependencyMutation::Remove("Numbers".into())).unwrap();
    assert!(updated.starts_with(MANIFEST));
    assert!(updated.contains("// dependency note\r\n"));
    assert!(updated.contains("// trailing comment\r\n"));
    assert!(updated.ends_with("dependency \"Other\" {\r\n  source = \"path\"\r\n  path = \"../with spaces\"\r\n}\r\n"));
    let parsed = beskid_analysis::projects::parse_manifest(&updated).unwrap();
    assert_eq!(parsed.dependencies.len(), 1);
    assert_eq!(parsed.dependencies[0].name, "Other");
    assert_eq!(EditManifest(&updated, &DependencyMutation::Remove("Numbers".into())).unwrap(), updated);
}

#[test]
fn dep06_path_intent_remains_relative_and_conflicts_fail() {
    let mutation = DependencyMutation::Add(DependencyIntent {
        name: "Local".into(),
        source: DependencyIntentSource::Path(PathBuf::from("../café project")),
    });
    let updated = EditManifest(MANIFEST, &mutation).unwrap();
    assert!(updated.contains("path = \"../café project\""));
    assert_eq!(EditManifest(&updated, &mutation).unwrap(), updated);
    assert!(EditManifest(&updated, &registry("Local", "1.0.0")).is_err());
}

#[test]
fn dep06_update_changes_only_selected_version_bytes() {
    let original = format!(
        "{MANIFEST}dependency \"Numbers\" {{\r\n  source = \"registry\"\r\n  version = \"1.2.3\" // version note\r\n}}\r\n"
    );
    let mutation =
        DependencyMutation::Update { package: Some("Numbers".into()), version: Some("1.2.4".into()), all: false };
    let updated = EditManifest(&original, &mutation).unwrap();
    assert_eq!(updated, original.replace("\"1.2.3\"", "\"1.2.4\""));
    assert!(EditManifest(&original, &DependencyMutation::Update { package: None, version: None, all: false }).is_err());
    assert!(
        EditManifest(
            &original,
            &DependencyMutation::Update { package: Some("Numbers".into()), version: None, all: true }
        )
        .is_err()
    );
}

#[test]
fn dep06_ambiguous_project_requires_explicit_selection() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("First.bproj");
    let second = temp.path().join("Second.bproj");
    std::fs::write(&first, MANIFEST).unwrap();
    std::fs::write(&second, MANIFEST).unwrap();
    let error = SelectDependencyProject(temp.path(), None).unwrap_err().to_string();
    assert!(error.contains("--project"));
    assert!(error.find("First.bproj").unwrap() < error.find("Second.bproj").unwrap());
    assert_eq!(SelectDependencyProject(temp.path(), Some(&first)).unwrap(), first);
}

#[test]
fn dep06_duplicate_dependencies_are_rejected_without_rewrite() {
    let original = format!(
        "{MANIFEST}dependency \"Numbers\" {{ source = \"registry\" version = \"1.2.3\" }}\ndependency \"Numbers\" {{ source = \"registry\" version = \"1.2.4\" }}\n"
    );
    assert!(EditManifest(&original, &DependencyMutation::Remove("Numbers".into())).is_err());
}

#[test]
fn dep06_escaped_intents_and_unresolved_versions_fail_closed() {
    assert!(EditManifest(MANIFEST, &registry("Unresolved", "")).is_err());
    let intent = DependencyMutation::Add(DependencyIntent {
        name: "Quotes\"andUnicode😀".into(),
        source: DependencyIntentSource::Registry { registry: Some("registry\"name".into()), version: "1.2.3".into() },
    });
    let updated = EditManifest(MANIFEST, &intent).unwrap();
    assert!(updated.starts_with(MANIFEST));
    let manifest = beskid_analysis::projects::parse_manifest(&updated).unwrap();
    assert_eq!(manifest.dependencies[0].name, "Quotes\"andUnicode😀");
    assert_eq!(manifest.dependencies[0].registry.as_deref(), Some("registry\"name"));
    assert_eq!(EditManifest(&updated, &intent).unwrap(), updated);
    let wrong_source = format!("{MANIFEST}dependency \"Unknown\" {{ source = \"invalid\" version = \"1.0.0\" }}");
    assert!(EditManifest(&wrong_source, &DependencyMutation::Remove("Unknown".into())).is_err());
}

#[test]
fn dep06_update_ignores_matching_versions_in_comments_and_other_blocks() {
    let original = format!(
        "{MANIFEST}// \"1.2.3\" is an example\r\ndependency \"Numbers\" {{ source = \"registry\" version = \"1.2.3\" }}\r\ndependency \"Other\" {{ source = \"registry\" version = \"1.2.3\" }}\r\n"
    );
    let mutation =
        DependencyMutation::Update { package: Some("Numbers".into()), version: Some("2.0.0".into()), all: false };
    let updated = EditManifest(&original, &mutation).unwrap();
    assert!(updated.contains("// \"1.2.3\" is an example"));
    assert!(updated.ends_with("dependency \"Other\" { source = \"registry\" version = \"1.2.3\" }\r\n"));
    let parsed = beskid_analysis::projects::parse_manifest(&updated).unwrap();
    assert_eq!(parsed.dependencies[0].version.as_deref(), Some("2.0.0"));
    assert_eq!(
        EditManifest(&updated, &DependencyMutation::Update { package: None, version: None, all: true }).unwrap(),
        updated
    );
}

#[test]
fn dep06_workspace_selection_does_not_pick_first_member() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["Zulu", "Alpha"] {
        let directory = temp.path().join(name);
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join(format!("{name}.bproj")), MANIFEST).unwrap();
    }
    let workspace = temp.path().join("Workspace.bws");
    std::fs::write(&workspace, "workspace { name = \"Workspace\" resolver = \"v1\" }\nmember \"Zulu\" { path = \"Zulu\" }\nmember \"Alpha\" { path = \"Alpha\" }\n").unwrap();
    let error = SelectDependencyProject(&workspace, None).unwrap_err().to_string();
    assert!(error.contains("--project"));
    assert!(error.find("Alpha.bproj").unwrap() < error.find("Zulu.bproj").unwrap());
    assert_eq!(SelectDependencyProject(&temp.path().join("Zulu"), None).unwrap(), temp.path().join("Zulu/Zulu.bproj"));
}
