use beskid_up::DirectInstall;
use semver::Version;
use tempfile::tempdir;

#[test]
fn activation_rejects_a_missing_payload() {
    let temp = tempdir().unwrap();
    let store = DirectInstall::new(temp.path());

    assert!(store.activate(&Version::parse("1.2.3").unwrap()).is_err());
}

#[test]
fn v06_activation_rejects_empty_payload_without_switching() {
    let temp = tempdir().unwrap();
    let store = DirectInstall::new(temp.path());
    let version = Version::parse("0.6.0").unwrap();
    std::fs::create_dir_all(temp.path().join("versions/0.6.0")).unwrap();
    assert!(store.activate(&version).is_err(), "a directory alone is not a verified complete toolchain");
    assert_eq!(store.active_version().unwrap(), None);
}
