use std::fs;

use super::{
    corelib_root, corelib_workspace_root, expected_corelib_workspace_sources, foundation_src,
};

#[test]
fn checked_in_corelib_template_has_manifest_and_prelude() {
    let template_root = corelib_root();
    let manifest = template_root.join("Project.proj");
    let prelude = template_root.join("src/Prelude.bd");

    assert!(
        manifest.is_file(),
        "missing corelib manifest: {}",
        manifest.display()
    );
    assert!(
        prelude.is_file(),
        "missing corelib prelude: {}",
        prelude.display()
    );
}

#[test]
fn checked_in_corelib_template_is_resolved_from_corelib_submodule() {
    let root = corelib_root();
    assert!(
        root.ends_with("beskid_corelib"),
        "expected beskid_corelib directory, got {}",
        root.display()
    );
}

#[test]
fn checked_in_corelib_workspace_declares_workspace_manifest() {
    let ws = corelib_workspace_root().join("Workspace.proj");
    assert!(
        ws.is_file(),
        "missing corelib workspace manifest: {}",
        ws.display()
    );
    let raw = std::fs::read_to_string(&ws).expect("read Workspace.proj");
    assert!(
        raw.contains("workspace {"),
        "Workspace.proj should open a workspace block"
    );
}

#[test]
fn checked_in_corelib_template_declares_corelib_project_name() {
    let root = corelib_root();
    let manifest = std::fs::read_to_string(root.join("Project.proj")).expect("read manifest");
    assert!(
        manifest.contains("name = \"corelib\""),
        "expected corelib package identity in Project.proj"
    );
}

#[test]
fn checked_in_corelib_template_has_mvp_module_files() {
    let root = corelib_workspace_root();

    for relative in expected_corelib_workspace_sources() {
        let path = root.join(relative);
        assert!(
            path.is_file(),
            "missing corelib source file: {}",
            path.display()
        );
    }
}

#[test]
fn compiler_sdk_syntax_node_files_track_inventory() {
    let inv_path = super::compiler_sdk_src().join("Beskid/Compiler/Syntax/Nodes/_inventory.txt");
    assert!(
        inv_path.is_file(),
        "missing syntax node inventory: {}",
        inv_path.display()
    );
    let raw = fs::read_to_string(&inv_path).expect("read _inventory.txt");
    let nodes_dir = inv_path.parent().expect("Nodes dir");
    for line in raw.lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        let node_file = nodes_dir.join(format!("{name}.bd"));
        assert!(
            node_file.is_file(),
            "inventory lists {name} but missing {}",
            node_file.display()
        );
    }
}

#[test]
fn checked_in_corelib_foundation_package_has_manifest() {
    let p = foundation_src().parent().expect("src").join("Project.proj");
    assert!(p.is_file(), "missing foundation manifest: {}", p.display());
}

#[test]
fn checked_in_corelib_template_has_beskid_tests_project() {
    let root = corelib_root();
    let tests_project = root.join("tests/corelib_tests/Project.proj");
    let write_tests = root.join("tests/corelib_tests/src/system/SyscallWriteTests.bd");
    let api_tests = root.join("tests/corelib_tests/src/system/SyscallApiTests.bd");
    assert!(
        tests_project.is_file(),
        "missing corelib tests project manifest: {}",
        tests_project.display()
    );
    assert!(
        write_tests.is_file(),
        "missing syscall write tests: {}",
        write_tests.display()
    );
    assert!(
        api_tests.is_file(),
        "missing syscall api tests: {}",
        api_tests.display()
    );
}
