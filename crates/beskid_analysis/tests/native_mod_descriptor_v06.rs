//! Descriptor V2 evidence inventories are closed and source paths are not traversal authority.
use beskid_analysis::mod_host::{mod_artifact_inventory, native_mod_inventory_identity, read_mod_artifact_descriptor};
use std::{collections::BTreeMap, fs};

#[test]
fn v06_native_mod_rejects_legacy_object_descriptor_before_loading() {
    let root = tempfile::tempdir().unwrap();
    let descriptor = root.path().join("mod.descriptor.json");
    fs::write(&descriptor, r#"{"schemaVersion":1,"packageId":"Legacy","objectFile":"mod.o"}"#).unwrap();
    let error = read_mod_artifact_descriptor(&descriptor, root.path()).unwrap_err();
    assert!(error.to_string().contains("V2"), "{error}");
}
#[test]
fn v06_native_mod_inventory_tracks_new_deleted_and_changed_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("payload"), b"first").unwrap();
    let first = mod_artifact_inventory(root.path()).unwrap();
    fs::write(root.path().join("payload"), b"second").unwrap();
    let second = mod_artifact_inventory(root.path()).unwrap();
    assert_ne!(first, second);
    fs::write(root.path().join("new"), b"new").unwrap();
    assert_ne!(second, mod_artifact_inventory(root.path()).unwrap());
    fs::remove_file(root.path().join("payload")).unwrap();
    assert!(!mod_artifact_inventory(root.path()).unwrap().contains_key("payload"));
}
#[test]
fn v06_native_mod_source_identity_is_framed_not_concatenated() {
    let a = BTreeMap::from([("ab".to_owned(), "c".to_owned())]);
    let b = BTreeMap::from([("a".to_owned(), "bc".to_owned())]);
    assert_ne!(native_mod_inventory_identity(&a), native_mod_inventory_identity(&b));
}
#[cfg(unix)]
#[test]
fn v06_native_mod_inventory_rejects_external_and_cyclic_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("private"), b"external bytes").unwrap();
    std::os::unix::fs::symlink(external.path().join("private"), root.path().join("external")).unwrap();
    assert!(mod_artifact_inventory(root.path()).is_err());
    fs::remove_file(root.path().join("external")).unwrap();
    std::os::unix::fs::symlink(root.path(), root.path().join("cycle")).unwrap();
    assert!(mod_artifact_inventory(root.path()).is_err());
    assert_eq!(fs::read(external.path().join("private")).unwrap(), b"external bytes");
}
