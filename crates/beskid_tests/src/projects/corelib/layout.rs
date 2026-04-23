use super::{corelib_root, expected_corelib_files};

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
fn checked_in_corelib_template_declares_corelib_project_name() {
    let root = corelib_root();
    let manifest = std::fs::read_to_string(root.join("Project.proj")).expect("read manifest");
    assert!(
        manifest.contains("name = \"beskid_corelib\""),
        "expected beskid_corelib project identity in Project.proj"
    );
}

#[test]
fn checked_in_corelib_template_has_mvp_module_files() {
    let root = corelib_root().join("src");

    for relative in expected_corelib_files() {
        let path = root.join(relative);
        assert!(
            path.is_file(),
            "missing corelib source file: {}",
            path.display()
        );
    }
}
