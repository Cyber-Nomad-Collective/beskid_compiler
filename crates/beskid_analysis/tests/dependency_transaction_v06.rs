use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use beskid_analysis::projects::dependency_transaction::{
    CommitDependencyPair, CommitDependencyPairWithIo, DependencyTransactionIo, NativeDependencyTransactionIo,
    RecoverDependencyPair,
};

const OLD_MANIFEST: &[u8] = b"manifest-original\r\n";
const NEW_MANIFEST: &[u8] = b"manifest-replacement\r\n";
const OLD_LOCK: &[u8] = b"lock-original\r\n";
const NEW_LOCK: &[u8] = b"lock-replacement\r\n";

fn fixture(with_lock: bool) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let manifest = fs::canonicalize(directory.path()).unwrap().join("App.bproj");
    fs::write(&manifest, OLD_MANIFEST).unwrap();
    if with_lock {
        fs::write(directory.path().join("Project.lock"), OLD_LOCK).unwrap();
    }
    (directory, manifest)
}
fn assert_pair(path: &Path, manifest: &[u8], lock: Option<&[u8]>) {
    assert_eq!(fs::read(path).unwrap(), manifest);
    match lock {
        Some(bytes) => assert_eq!(fs::read(path.with_file_name("Project.lock")).unwrap(), bytes),
        None => assert!(!path.with_file_name("Project.lock").exists()),
    }
}

struct FailIo {
    destination: &'static str,
    once: AtomicBool,
    sync_file: AtomicUsize,
    fail_sync: usize,
}
impl DependencyTransactionIo for FailIo {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        if destination.file_name().unwrap() == self.destination && !self.once.swap(true, Ordering::SeqCst) {
            return Err(io::Error::other("injected replacement failure"));
        }
        NativeDependencyTransactionIo.Replace(source, destination)
    }
    fn SyncFile(&self, file: &File) -> io::Result<()> {
        if self.fail_sync != 0 && self.sync_file.fetch_add(1, Ordering::SeqCst) + 1 == self.fail_sync {
            return Err(io::Error::other("injected durable file sync failure"));
        }
        NativeDependencyTransactionIo.SyncFile(file)
    }
    fn SyncDirectory(&self, path: &Path) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncDirectory(path)
    }
}

#[test]
fn dep06_pair_success_and_missing_lock_state() {
    for has_lock in [false, true] {
        let (_directory, manifest) = fixture(has_lock);
        CommitDependencyPair(&manifest, OLD_MANIFEST, has_lock.then_some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK).unwrap();
        assert_pair(&manifest, NEW_MANIFEST, Some(NEW_LOCK));
        assert!(!RecoverDependencyPair(&manifest).unwrap());
    }
}

#[test]
fn dep06_pair_rejects_concurrent_manifest_and_lock_bytes() {
    for edit_manifest in [false, true] {
        let (_directory, manifest) = fixture(true);
        let target = if edit_manifest { manifest.clone() } else { manifest.with_file_name("Project.lock") };
        fs::write(&target, b"external edit").unwrap();
        assert!(CommitDependencyPair(&manifest, OLD_MANIFEST, Some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"external edit");
        assert_eq!(
            fs::read(if edit_manifest { manifest.with_file_name("Project.lock") } else { manifest }).unwrap(),
            if edit_manifest { OLD_LOCK } else { OLD_MANIFEST }
        );
    }
}

#[test]
fn dep06_pair_restores_every_replacement_and_durability_failure() {
    for has_lock in [false, true] {
        for (destination, fail_sync) in [("App.bproj", 0), ("Project.lock", 0), ("", 1), ("", 2), ("", 3)] {
            let (_directory, manifest) = fixture(has_lock);
            let io = FailIo { destination, once: AtomicBool::new(false), sync_file: AtomicUsize::new(0), fail_sync };
            assert!(
                CommitDependencyPairWithIo(
                    &manifest,
                    OLD_MANIFEST,
                    has_lock.then_some(OLD_LOCK),
                    NEW_MANIFEST,
                    NEW_LOCK,
                    &io
                )
                .is_err()
            );
            assert_pair(&manifest, OLD_MANIFEST, has_lock.then_some(OLD_LOCK));
            // Recovery is safe and idempotent whether rollback already cleared the journal.
            RecoverDependencyPair(&manifest).unwrap();
            assert_pair(&manifest, OLD_MANIFEST, has_lock.then_some(OLD_LOCK));
        }
    }
}

struct ReentrantIo {
    manifest: PathBuf,
    saw_guard: AtomicBool,
}
impl DependencyTransactionIo for ReentrantIo {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        if destination == self.manifest {
            let error = CommitDependencyPair(&self.manifest, OLD_MANIFEST, Some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK)
                .unwrap_err()
                .to_string();
            assert!(error.contains("transaction") && error.contains("busy"), "{error}");
            self.saw_guard.store(true, Ordering::SeqCst);
        }
        NativeDependencyTransactionIo.Replace(source, destination)
    }
    fn SyncFile(&self, file: &File) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncFile(file)
    }
    fn SyncDirectory(&self, path: &Path) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncDirectory(path)
    }
}
#[test]
fn dep06_pair_guard_excludes_a_second_commit() {
    let (_directory, manifest) = fixture(true);
    let io = ReentrantIo { manifest: manifest.clone(), saw_guard: AtomicBool::new(false) };
    CommitDependencyPairWithIo(&manifest, OLD_MANIFEST, Some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK, &io).unwrap();
    assert!(io.saw_guard.load(Ordering::SeqCst));
    assert_pair(&manifest, NEW_MANIFEST, Some(NEW_LOCK));
}

struct CrashIo;
impl DependencyTransactionIo for CrashIo {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        NativeDependencyTransactionIo.Replace(source, destination)?;
        if destination.extension().is_some_and(|extension| extension == "bproj") {
            std::process::exit(91);
        }
        Ok(())
    }
    fn SyncFile(&self, file: &File) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncFile(file)
    }
    fn SyncDirectory(&self, path: &Path) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncDirectory(path)
    }
}
fn crash_child(manifest: &Path, has_lock: bool) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("dep06_pair_recovers_process_death_after_first_replace")
        .env("BESKID_DEPENDENCY_CRASH_MANIFEST", manifest)
        .env("BESKID_DEPENDENCY_CRASH_LOCK", if has_lock { "yes" } else { "no" })
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(91));
}
#[test]
fn dep06_pair_recovers_process_death_after_first_replace() {
    if let Some(path) = std::env::var_os("BESKID_DEPENDENCY_CRASH_MANIFEST") {
        let has_lock = std::env::var("BESKID_DEPENDENCY_CRASH_LOCK").unwrap() == "yes";
        CommitDependencyPairWithIo(
            Path::new(&path),
            OLD_MANIFEST,
            has_lock.then_some(OLD_LOCK),
            NEW_MANIFEST,
            NEW_LOCK,
            &CrashIo,
        )
        .unwrap();
        panic!("crash seam did not exit");
    }
    for has_lock in [false, true] {
        let (_directory, manifest) = fixture(has_lock);
        crash_child(&manifest, has_lock);
        assert_eq!(fs::read(&manifest).unwrap(), NEW_MANIFEST);
        assert!(RecoverDependencyPair(&manifest).unwrap());
        assert_pair(&manifest, OLD_MANIFEST, has_lock.then_some(OLD_LOCK));
        assert!(!RecoverDependencyPair(&manifest).unwrap());
    }
}

#[test]
fn dep06_recovery_preserves_external_bytes_after_interruption() {
    let (_directory, manifest) = fixture(true);
    crash_child(&manifest, true);
    fs::write(manifest.with_file_name("Project.lock"), b"external lock after crash").unwrap();
    let error = RecoverDependencyPair(&manifest).unwrap_err().to_string();
    assert!(error.contains("recovery"), "{error}");
    assert_pair(&manifest, NEW_MANIFEST, Some(b"external lock after crash"));
}

#[cfg(unix)]
#[test]
fn dep06_pair_rejects_symlink_destinations() {
    let (directory, manifest) = fixture(false);
    let unrelated = directory.path().join("unrelated");
    fs::write(&unrelated, b"preserve unrelated bytes").unwrap();
    std::os::unix::fs::symlink(&unrelated, manifest.with_file_name("Project.lock")).unwrap();
    assert!(CommitDependencyPair(&manifest, OLD_MANIFEST, None, NEW_MANIFEST, NEW_LOCK).is_err());
    assert_eq!(fs::read(&unrelated).unwrap(), b"preserve unrelated bytes");
    assert_eq!(fs::read(&manifest).unwrap(), OLD_MANIFEST);
}

struct FailVisibleSync {
    manifest: PathBuf,
    once: AtomicBool,
}
impl DependencyTransactionIo for FailVisibleSync {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        NativeDependencyTransactionIo.Replace(source, destination)
    }
    fn SyncFile(&self, file: &File) -> io::Result<()> {
        NativeDependencyTransactionIo.SyncFile(file)
    }
    fn SyncDirectory(&self, path: &Path) -> io::Result<()> {
        if path == self.manifest.parent().unwrap()
            && fs::read(&self.manifest).unwrap() == NEW_MANIFEST
            && !self.once.swap(true, Ordering::SeqCst)
        {
            return Err(io::Error::other("injected sync failure after visible manifest replacement"));
        }
        NativeDependencyTransactionIo.SyncDirectory(path)
    }
}
#[test]
fn dep06_directory_sync_failure_after_visible_write_restores_pair() {
    for has_lock in [false, true] {
        let (_directory, manifest) = fixture(has_lock);
        let io = FailVisibleSync { manifest: manifest.clone(), once: AtomicBool::new(false) };
        assert!(
            CommitDependencyPairWithIo(
                &manifest,
                OLD_MANIFEST,
                has_lock.then_some(OLD_LOCK),
                NEW_MANIFEST,
                NEW_LOCK,
                &io
            )
            .is_err()
        );
        assert!(io.once.load(Ordering::SeqCst));
        assert_pair(&manifest, OLD_MANIFEST, has_lock.then_some(OLD_LOCK));
    }
}

#[test]
fn dep06_corrupt_journal_fails_closed_without_changing_files() {
    let (directory, manifest) = fixture(true);
    crash_child(&manifest, true);
    let journal = directory.path().join(".beskid/dependency-transaction/journal.json");
    fs::write(&journal, b"not a valid journal").unwrap();
    let error = RecoverDependencyPair(&manifest).unwrap_err().to_string();
    assert!(error.contains("recovery") && error.contains("journal"), "{error}");
    assert_pair(&manifest, NEW_MANIFEST, Some(OLD_LOCK));
    assert_eq!(fs::read(&journal).unwrap(), b"not a valid journal");
}

#[cfg(unix)]
#[test]
fn dep06_journal_directory_and_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (directory, manifest) = fixture(true);
    crash_child(&manifest, true);
    let private = directory.path().join(".beskid/dependency-transaction");
    assert_eq!(fs::metadata(&private).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(private.join("journal.json")).unwrap().permissions().mode() & 0o077, 0);
    assert_eq!(fs::metadata(private.join("guard")).unwrap().permissions().mode() & 0o077, 0);
    RecoverDependencyPair(&manifest).unwrap();
}

#[cfg(unix)]
#[test]
fn dep06_public_pair_preserves_existing_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, manifest) = fixture(true);
    let lock = manifest.with_file_name("Project.lock");
    fs::set_permissions(&manifest, fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o640)).unwrap();
    CommitDependencyPair(&manifest, OLD_MANIFEST, Some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK).unwrap();
    assert_eq!(fs::metadata(&manifest).unwrap().permissions().mode() & 0o777, 0o644);
    assert_eq!(fs::metadata(&lock).unwrap().permissions().mode() & 0o777, 0o640);
}

#[cfg(unix)]
#[test]
fn dep06_rollback_preserves_existing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, manifest) = fixture(true);
    let lock = manifest.with_file_name("Project.lock");
    fs::set_permissions(&manifest, fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o640)).unwrap();
    let io = FailIo {
        destination: "Project.lock",
        once: AtomicBool::new(false),
        sync_file: AtomicUsize::new(0),
        fail_sync: 0,
    };
    assert!(CommitDependencyPairWithIo(&manifest, OLD_MANIFEST, Some(OLD_LOCK), NEW_MANIFEST, NEW_LOCK, &io).is_err());
    assert_pair(&manifest, OLD_MANIFEST, Some(OLD_LOCK));
    assert_eq!(fs::metadata(&manifest).unwrap().permissions().mode() & 0o777, 0o644);
    assert_eq!(fs::metadata(&lock).unwrap().permissions().mode() & 0o777, 0o640);
}

#[cfg(unix)]
#[test]
fn dep06_new_lock_uses_ordinary_file_creation_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (directory, manifest) = fixture(false);
    let ordinary = directory.path().join("ordinary-file");
    fs::write(&ordinary, b"ordinary").unwrap();
    let expected = fs::metadata(&ordinary).unwrap().permissions().mode() & 0o777;
    CommitDependencyPair(&manifest, OLD_MANIFEST, None, NEW_MANIFEST, NEW_LOCK).unwrap();
    assert_eq!(fs::metadata(manifest.with_file_name("Project.lock")).unwrap().permissions().mode() & 0o777, expected);
}
