//! Durable, guarded manifest/lock commits. This module does not resolve dependencies.
#![allow(non_snake_case)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::ProjectError;

const JOURNAL_LIMIT: u64 = 256 * 1024 * 1024;

/// Actual filesystem boundary used by both native commits and fault-injection consumers.
pub trait DependencyTransactionIo {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    fn SyncFile(&self, file: &File) -> io::Result<()>;
    fn SyncDirectory(&self, directory: &Path) -> io::Result<()>;
}

pub struct NativeDependencyTransactionIo;
impl DependencyTransactionIo for NativeDependencyTransactionIo {
    fn Replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(source, destination)
    }
    fn SyncFile(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
    fn SyncDirectory(&self, directory: &Path) -> io::Result<()> {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new().read(true).custom_flags(0x02000000).open(directory)?
        };
        #[cfg(not(windows))]
        let file = File::open(directory)?;
        file.sync_all()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    project_directory: PathBuf,
    manifest_name: String,
    state: CommitState,
    original_manifest: Vec<u8>,
    original_lock: Option<Vec<u8>>,
    replacement_manifest: Vec<u8>,
    replacement_lock: Vec<u8>,
    replacement_manifest_identity: FileIdentity,
    replacement_lock_identity: FileIdentity,
}
#[derive(Serialize, Deserialize, PartialEq)]
enum CommitState {
    Prepared,
    Committed,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    creation_time: u64,
    #[cfg(windows)]
    last_write_time: u64,
    length: u64,
}
impl FileIdentity {
    fn Read(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;
        Ok(Self {
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(windows)]
            creation_time: metadata.creation_time(),
            #[cfg(windows)]
            last_write_time: metadata.last_write_time(),
            length: metadata.len(),
        })
    }
}

struct Guard {
    file: File,
    directory: PathBuf,
    private: PathBuf,
    manifest: PathBuf,
    lock: PathBuf,
    journal: PathBuf,
}
impl Guard {
    fn Acquire(manifest_path: &Path) -> Result<Self, ProjectError> {
        let parent = manifest_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
        RejectSymlink(parent)?;
        let directory = fs::canonicalize(parent).map_err(Io)?;
        let name = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Failure("manifest filename must be valid Unicode"))?;
        if name == "Project.lock" {
            return Err(Failure("manifest and Project.lock destinations must differ"));
        }
        let manifest = directory.join(name);
        let lock = directory.join("Project.lock");
        let internal = directory.join(".beskid");
        SecureDirectory(&internal)?;
        let private = internal.join("dependency-transaction");
        SecureDirectory(&private)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if fs::metadata(&private).map_err(Io)?.dev() != fs::metadata(&directory).map_err(Io)?.dev() {
                return Err(Failure("transaction staging must be on the project filesystem"));
            }
        }
        let guard_path = private.join("guard");
        CheckFile(&guard_path, true)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&guard_path).map_err(Io)?;
        CheckFile(&guard_path, false)?;
        file.try_lock_exclusive()
            .map_err(|error| Failure(format!("dependency transaction busy at {}: {error}", guard_path.display())))?;
        let guard =
            Self { file, directory, private: private.clone(), manifest, lock, journal: private.join("journal.json") };
        CheckFile(&guard.manifest, false)?;
        CheckFile(&guard.lock, true)?;
        Ok(guard)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn CommitDependencyPair(
    manifest_path: &Path,
    original_manifest: &[u8],
    original_lock: Option<&[u8]>,
    replacement_manifest: &[u8],
    replacement_lock: &[u8],
) -> Result<(), ProjectError> {
    CommitDependencyPairWithIo(
        manifest_path,
        original_manifest,
        original_lock,
        replacement_manifest,
        replacement_lock,
        &NativeDependencyTransactionIo,
    )
}

pub fn CommitDependencyPairWithIo(
    manifest_path: &Path,
    original_manifest: &[u8],
    original_lock: Option<&[u8]>,
    replacement_manifest: &[u8],
    replacement_lock: &[u8],
    io: &dyn DependencyTransactionIo,
) -> Result<(), ProjectError> {
    let guard = Guard::Acquire(manifest_path)?;
    Recover(&guard)?;
    Expected(&guard.manifest, Some(original_manifest))?;
    Expected(&guard.lock, original_lock)?;
    let manifest_stage = StagePublished(&guard.private, &guard.manifest, replacement_manifest, io)?;
    let lock_stage = StagePublished(&guard.private, &guard.lock, replacement_lock, io)?;
    let mut journal = Journal {
        schema_version: 1,
        project_directory: guard.directory.clone(),
        manifest_name: guard.manifest.file_name().unwrap().to_str().unwrap().to_owned(),
        state: CommitState::Prepared,
        original_manifest: original_manifest.to_vec(),
        original_lock: original_lock.map(<[u8]>::to_vec),
        replacement_manifest: replacement_manifest.to_vec(),
        replacement_lock: replacement_lock.to_vec(),
        replacement_manifest_identity: FileIdentity::Read(manifest_stage.path()).map_err(Io)?,
        replacement_lock_identity: FileIdentity::Read(lock_stage.path()).map_err(Io)?,
    };
    // A complete durable original/staged pair precedes any visible replacement.
    SaveJournal(&guard, &journal, io)?;
    let result: Result<(), ProjectError> = (|| {
        Expected(&guard.manifest, Some(original_manifest))?;
        Expected(&guard.lock, original_lock)?;
        io.Replace(manifest_stage.path(), &guard.manifest).map_err(Io)?;
        io.SyncDirectory(&guard.directory).map_err(Io)?;
        Expected(&guard.manifest, Some(replacement_manifest))?;
        Expected(&guard.lock, original_lock)?;
        io.Replace(lock_stage.path(), &guard.lock).map_err(Io)?;
        io.SyncDirectory(&guard.directory).map_err(Io)?;
        Expected(&guard.manifest, Some(replacement_manifest))?;
        Expected(&guard.lock, Some(replacement_lock))?;
        journal.state = CommitState::Committed;
        SaveJournal(&guard, &journal, io)?;
        ClearJournal(&guard)?;
        Ok(())
    })();
    if let Err(error) = result {
        journal.state = CommitState::Prepared;
        // Retain rollback intent durably even if failure happened during final journal cleanup.
        if let Err(journal_error) = SaveJournal(&guard, &journal, &NativeDependencyTransactionIo) {
            return Err(RecoveryGuidance(
                &guard,
                format!("{error}; could not retain rollback journal: {journal_error}"),
            ));
        }
        if let Err(rollback_error) = Restore(&guard, &journal) {
            return Err(RecoveryGuidance(&guard, format!("{error}; rollback failed: {rollback_error}")));
        }
        return Err(error);
    }
    Ok(())
}

/// Recover before project reads; never overwrite bytes outside the recorded original/staged pair.
pub fn RecoverDependencyPair(manifest_path: &Path) -> Result<bool, ProjectError> {
    let guard = Guard::Acquire(manifest_path)?;
    Recover(&guard)
}

fn Recover(guard: &Guard) -> Result<bool, ProjectError> {
    if !guard.journal.try_exists().map_err(Io)? {
        return Ok(false);
    }
    CheckFile(&guard.journal, false)?;
    let mut bytes = Vec::new();
    File::open(&guard.journal).map_err(Io)?.take(JOURNAL_LIMIT + 1).read_to_end(&mut bytes).map_err(Io)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(RecoveryGuidance(guard, "journal exceeds the recovery size limit"));
    }
    let journal: Journal =
        serde_json::from_slice(&bytes).map_err(|error| RecoveryGuidance(guard, format!("invalid journal: {error}")))?;
    if journal.schema_version != 1
        || journal.project_directory != guard.directory
        || Some(journal.manifest_name.as_str()) != guard.manifest.file_name().and_then(|name| name.to_str())
    {
        return Err(RecoveryGuidance(guard, "journal project identity/version mismatch"));
    }
    if journal.state == CommitState::Committed {
        Expected(&guard.manifest, Some(&journal.replacement_manifest))
            .map_err(|error| RecoveryGuidance(guard, error))?;
        Expected(&guard.lock, Some(&journal.replacement_lock)).map_err(|error| RecoveryGuidance(guard, error))?;
        ClearJournal(guard)?;
    } else {
        Restore(guard, &journal).map_err(|error| RecoveryGuidance(guard, error))?;
    }
    Ok(true)
}

fn Restore(guard: &Guard, journal: &Journal) -> Result<(), ProjectError> {
    // Check BOTH before changing either; an external editor owns any unrecognized bytes.
    Recoverable(
        &guard.manifest,
        Some(&journal.original_manifest),
        &journal.replacement_manifest,
        &journal.replacement_manifest_identity,
    )?;
    Recoverable(
        &guard.lock,
        journal.original_lock.as_deref(),
        &journal.replacement_lock,
        &journal.replacement_lock_identity,
    )?;
    RestoreOne(
        guard,
        &guard.manifest,
        Some(&journal.original_manifest),
        &journal.replacement_manifest,
        &journal.replacement_manifest_identity,
    )?;
    RestoreOne(
        guard,
        &guard.lock,
        journal.original_lock.as_deref(),
        &journal.replacement_lock,
        &journal.replacement_lock_identity,
    )?;
    ClearJournal(guard)
}
fn RestoreOne(
    guard: &Guard,
    path: &Path,
    original: Option<&[u8]>,
    replacement: &[u8],
    identity: &FileIdentity,
) -> Result<(), ProjectError> {
    Recoverable(path, original, replacement, identity)?;
    if ReadOptional(path)?.as_deref() == original {
        return Ok(());
    }
    match original {
        Some(bytes) => {
            let stage = StagePublished(&guard.private, path, bytes, &NativeDependencyTransactionIo)?;
            Recoverable(path, original, replacement, identity)?;
            NativeDependencyTransactionIo.Replace(stage.path(), path).map_err(Io)?;
        }
        None => {
            Recoverable(path, original, replacement, identity)?;
            fs::remove_file(path).map_err(Io)?;
        }
    }
    NativeDependencyTransactionIo.SyncDirectory(&guard.directory).map_err(Io)
}
fn Recoverable(
    path: &Path,
    original: Option<&[u8]>,
    replacement: &[u8],
    identity: &FileIdentity,
) -> Result<(), ProjectError> {
    let current = ReadOptional(path)?;
    if current.as_deref() == original {
        return Ok(());
    }
    if current.as_deref() == Some(replacement) && FileIdentity::Read(path).map_err(Io)? == *identity {
        return Ok(());
    }
    Err(Failure(format!("external bytes or file identity changed at {}; refusing recovery overwrite", path.display())))
}
fn Expected(path: &Path, bytes: Option<&[u8]>) -> Result<(), ProjectError> {
    if ReadOptional(path)?.as_deref() != bytes {
        return Err(Failure(format!(
            "concurrent dependency edit at {}; planned bytes no longer match",
            path.display()
        )));
    }
    Ok(())
}
fn ReadOptional(path: &Path) -> Result<Option<Vec<u8>>, ProjectError> {
    CheckFile(path, true)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Io(error)),
    }
}
fn Stage(directory: &Path, bytes: &[u8], io: &dyn DependencyTransactionIo) -> Result<NamedTempFile, ProjectError> {
    let mut file = tempfile::Builder::new().prefix("stage-").tempfile_in(directory).map_err(Io)?;
    file.write_all(bytes).map_err(Io)?;
    io.SyncFile(file.as_file()).map_err(Io)?;
    Ok(file)
}
// Public files retain destination permissions. New Unix files use ordinary
// 0666 creation filtered by the process umask, without changing that umask.
fn StagePublished(
    directory: &Path,
    destination: &Path,
    bytes: &[u8],
    io: &dyn DependencyTransactionIo,
) -> Result<NamedTempFile, ProjectError> {
    let existing = match fs::metadata(destination) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(Io(error)),
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix("stage-");
    #[cfg(unix)]
    if existing.is_none() {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(directory).map_err(Io)?;
    if let Some(permissions) = existing {
        file.as_file().set_permissions(permissions).map_err(Io)?;
    }
    file.write_all(bytes).map_err(Io)?;
    io.SyncFile(file.as_file()).map_err(Io)?;
    Ok(file)
}
fn SaveJournal(guard: &Guard, journal: &Journal, io: &dyn DependencyTransactionIo) -> Result<(), ProjectError> {
    let bytes =
        serde_json::to_vec(journal).map_err(|error| Failure(format!("journal serialization failed: {error}")))?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(Failure("transaction journal exceeds size limit"));
    }
    let stage = Stage(&guard.private, &bytes, io)?;
    CheckFile(&guard.journal, true)?;
    io.Replace(stage.path(), &guard.journal).map_err(Io)?;
    io.SyncDirectory(&guard.private).map_err(Io)?;
    // Ensure discovery of the journal directory is also durable on first use.
    io.SyncDirectory(guard.private.parent().unwrap()).map_err(Io)?;
    io.SyncDirectory(&guard.directory).map_err(Io)
}
fn ClearJournal(guard: &Guard) -> Result<(), ProjectError> {
    fs::remove_file(&guard.journal).map_err(Io)?;
    NativeDependencyTransactionIo.SyncDirectory(&guard.private).map_err(Io)
}
fn RejectSymlink(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(Io)?;
    if metadata.file_type().is_symlink() {
        return Err(Failure(format!("symlink transaction destination rejected: {}", path.display())));
    }
    Ok(())
}
fn CheckFile(path: &Path, missing_allowed: bool) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(Failure(format!(
                    "non-regular or symlink transaction destination rejected: {}",
                    path.display()
                )));
            }
            CheckOwner(path, &metadata)?;
            Ok(())
        }
        Err(error) if missing_allowed && error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Io(error)),
    }
}
fn SecureDirectory(path: &Path) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(Failure(format!("unsafe transaction directory: {}", path.display())));
            }
            CheckOwner(path, &metadata)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if path.file_name().is_some_and(|name| name == "dependency-transaction") && metadata.mode() & 0o077 != 0
                {
                    return Err(Failure(format!("transaction directory must be private (0700): {}", path.display())));
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path).map_err(Io)?;
        }
        Err(error) => return Err(Io(error)),
    }
    Ok(())
}
fn CheckOwner(path: &Path, metadata: &fs::Metadata) -> Result<(), ProjectError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Creator-owned anonymous file supplies the effective filesystem UID without unsafe FFI.
        let creator = tempfile::tempfile().map_err(Io)?.metadata().map_err(Io)?.uid();
        if metadata.uid() != creator {
            return Err(Failure(format!("foreign-owned transaction destination rejected: {}", path.display())));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, metadata);
    }
    Ok(())
}
fn Failure(message: impl std::fmt::Display) -> ProjectError {
    ProjectError::Validation(message.to_string())
}
fn Io(error: io::Error) -> ProjectError {
    Failure(format!("dependency transaction I/O failure: {error}"))
}
fn RecoveryGuidance(guard: &Guard, message: impl std::fmt::Display) -> ProjectError {
    Failure(format!(
        "dependency transaction recovery required at {}: {message}; preserve journal and both files, resolve external edits, then rerun recovery before project reads",
        guard.journal.display()
    ))
}
