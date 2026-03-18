use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::projects::{
    PROJECT_FILE_NAME, WORKSPACE_FILE_NAME, discover_project_file, discover_workspace_file,
};

fn temp_case_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_projects_discovery_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn discovery_finds_project_file_upwards() {
    let root = temp_case_dir("upward");
    let nested = root.join("Src").join("Net");
    fs::create_dir_all(&nested).expect("create nested");

    let manifest = root.join(PROJECT_FILE_NAME);
    fs::write(&manifest, "project { name = \"A\" version = \"0.1.0\" }\n").expect("write manifest");

    let found = discover_project_file(&nested).expect("must find project file");
    assert_eq!(found, manifest);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_finds_workspace_file_upwards() {
    let root = temp_case_dir("workspace_upward");
    let nested = root.join("compiler").join("Src");
    fs::create_dir_all(&nested).expect("create nested");

    let workspace_manifest = root.join(WORKSPACE_FILE_NAME);
    fs::write(
        &workspace_manifest,
        "workspace {\n  name = \"Root\"\n}\nmember \"compiler\" {\n  path = \"compiler\"\n}\n",
    )
    .expect("write workspace manifest");

    let found = discover_workspace_file(&nested).expect("must find workspace file");
    assert_eq!(found, workspace_manifest);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn workspace_discovery_returns_none_when_missing() {
    let root = temp_case_dir("workspace_missing");
    let nested = root.join("compiler").join("Src");
    fs::create_dir_all(&nested).expect("create nested");

    let found = discover_workspace_file(&nested);
    assert!(found.is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_returns_none_when_missing() {
    let root = temp_case_dir("missing");
    let nested = root.join("Src").join("Net");
    fs::create_dir_all(&nested).expect("create nested");

    let found = discover_project_file(&nested);
    assert!(found.is_none());

    let _ = fs::remove_dir_all(root);
}
