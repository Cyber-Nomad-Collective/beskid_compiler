use beskid_up::DirectInstall;
use semver::Version;
use tempfile::tempdir;

#[test]
fn activation_switches_between_verified_versions() {
    let temp = tempdir().unwrap();
    let store = DirectInstall::new(temp.path());
    let first = Version::parse("1.2.3").unwrap();
    let second = Version::parse("1.3.0").unwrap();

    store.install_empty(&first).unwrap();
    store.install_empty(&second).unwrap();
    store.activate(&first).unwrap();
    assert_eq!(store.active_version().unwrap(), Some(first));

    store.activate(&second).unwrap();
    assert_eq!(store.active_version().unwrap(), Some(second));
}

#[test]
fn activation_rejects_a_missing_payload() {
    let temp = tempdir().unwrap();
    let store = DirectInstall::new(temp.path());

    assert!(store.activate(&Version::parse("1.2.3").unwrap()).is_err());
}
