use super::{expected_stdlib_files, stdlib_root};

#[test]
fn checked_in_stdlib_template_has_manifest_and_prelude() {
    let template_root = stdlib_root();
    let manifest = template_root.join("Project.proj");
    let prelude = template_root.join("src/Prelude.bd");

    assert!(manifest.is_file(), "missing stdlib manifest: {}", manifest.display());
    assert!(prelude.is_file(), "missing stdlib prelude: {}", prelude.display());
}

#[test]
fn checked_in_stdlib_template_has_all_module_files() {
    let root = stdlib_root().join("src");

    for relative in expected_stdlib_files() {
        let path = root.join(relative);
        assert!(path.is_file(), "missing stdlib source file: {}", path.display());
    }
}
