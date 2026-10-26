//! Durable, guarded manifest/lock commits. This module does not resolve dependencies.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tempfile::TempPath;

use super::ProjectError;

const JOURNAL_LIMIT: u64 = 256 * 1024 * 1024;

/// Actual filesystem boundary used by both native commits and fault-injection consumers.
pub trait DependencyTransactionIo {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    fn sync_file(&self, file: &File) -> io::Result<()>;
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;
}

pub struct NativeDependencyTransactionIo;
impl DependencyTransactionIo for NativeDependencyTransactionIo {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(source, destination)
    }
    fn sync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        // FlushFileBuffers requires a handle with GENERIC_WRITE access; a read-only directory
        // handle fails with ERROR_ACCESS_DENIED. FILE_FLAG_BACKUP_SEMANTICS opens the directory.
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
            OpenOptions::new().write(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(directory)?
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
    fn read(path: &Path) -> io::Result<Self> {
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
    fn acquire(manifest_path: &Path) -> Result<Self, ProjectError> {
        let parent = manifest_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
        reject_symlink(parent)?;
        let directory = fs::canonicalize(parent).map_err(io_error)?;
        let name = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| failure("manifest filename must be valid Unicode"))?;
        if name == "Project.lock" {
            return Err(failure("manifest and Project.lock destinations must differ"));
        }
        let manifest = directory.join(name);
        let lock = directory.join("Project.lock");
        let internal = directory.join(".beskid");
        secure_directory(&internal)?;
        let private = internal.join("dependency-transaction");
        secure_directory(&private)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if fs::metadata(&private).map_err(io_error)?.dev() != fs::metadata(&directory).map_err(io_error)?.dev() {
                return Err(failure("transaction staging must be on the project filesystem"));
            }
        }
        let guard_path = private.join("guard");
        check_file(&guard_path, true)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&guard_path).map_err(io_error)?;
        check_file(&guard_path, false)?;
        file.try_lock_exclusive()
            .map_err(|error| failure(format!("dependency transaction busy at {}: {error}", guard_path.display())))?;
        let guard =
            Self { file, directory, private: private.clone(), manifest, lock, journal: private.join("journal.json") };
        check_file(&guard.manifest, false)?;
        check_file(&guard.lock, true)?;
        Ok(guard)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn commit_dependency_pair(
    manifest_path: &Path,
    original_manifest: &[u8],
    original_lock: Option<&[u8]>,
    replacement_manifest: &[u8],
    replacement_lock: &[u8],
) -> Result<(), ProjectError> {
    commit_dependency_pair_with_io(
        manifest_path,
        original_manifest,
        original_lock,
        replacement_manifest,
        replacement_lock,
        &NativeDependencyTransactionIo,
    )
}

pub fn commit_dependency_pair_with_io(
    manifest_path: &Path,
    original_manifest: &[u8],
    original_lock: Option<&[u8]>,
    replacement_manifest: &[u8],
    replacement_lock: &[u8],
    io: &dyn DependencyTransactionIo,
) -> Result<(), ProjectError> {
    let guard = Guard::acquire(manifest_path)?;
    recover(&guard)?;
    expected(&guard.manifest, Some(original_manifest))?;
    expected(&guard.lock, original_lock)?;
    let manifest_stage = stage_published(&guard.private, &guard.manifest, replacement_manifest, io)?;
    let lock_stage = stage_published(&guard.private, &guard.lock, replacement_lock, io)?;
    let mut journal = Journal {
        schema_version: 1,
        project_directory: guard.directory.clone(),
        manifest_name: guard.manifest.file_name().unwrap().to_str().unwrap().to_owned(),
        state: CommitState::Prepared,
        original_manifest: original_manifest.to_vec(),
        original_lock: original_lock.map(<[u8]>::to_vec),
        replacement_manifest: replacement_manifest.to_vec(),
        replacement_lock: replacement_lock.to_vec(),
        replacement_manifest_identity: FileIdentity::read(&manifest_stage).map_err(io_error)?,
        replacement_lock_identity: FileIdentity::read(&lock_stage).map_err(io_error)?,
    };
    // A complete durable original/staged pair precedes any visible replacement.
    save_journal(&guard, &journal, io)?;
    let result: Result<(), ProjectError> = (|| {
        expected(&guard.manifest, Some(original_manifest))?;
        expected(&guard.lock, original_lock)?;
        io.replace(&manifest_stage, &guard.manifest).map_err(io_error)?;
        io.sync_directory(&guard.directory).map_err(io_error)?;
        expected(&guard.manifest, Some(replacement_manifest))?;
        expected(&guard.lock, original_lock)?;
        io.replace(&lock_stage, &guard.lock).map_err(io_error)?;
        io.sync_directory(&guard.directory).map_err(io_error)?;
        expected(&guard.manifest, Some(replacement_manifest))?;
        expected(&guard.lock, Some(replacement_lock))?;
        journal.state = CommitState::Committed;
        save_journal(&guard, &journal, io)?;
        clear_journal(&guard)?;
        Ok(())
    })();
    if let Err(error) = result {
        journal.state = CommitState::Prepared;
        // Retain rollback intent durably even if failure happened during final journal cleanup.
        if let Err(journal_error) = save_journal(&guard, &journal, &NativeDependencyTransactionIo) {
            return Err(recovery_guidance(
                &guard,
                format!("{error}; could not retain rollback journal: {journal_error}"),
            ));
        }
        if let Err(rollback_error) = restore(&guard, &journal) {
            return Err(recovery_guidance(&guard, format!("{error}; rollback failed: {rollback_error}")));
        }
        return Err(error);
    }
    Ok(())
}

/// Recover an interrupted commit of `manifest_path` before any project read (DEP06-05). Only a project
/// that has a commit journal is touched: no private state is created for an ordinary read.
pub fn recover_interrupted_dependency_pair(manifest_path: &Path) -> Result<bool, ProjectError> {
    let parent = manifest_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let journal = parent.join(".beskid").join("dependency-transaction").join("journal.json");
    match fs::symlink_metadata(&journal) {
        Ok(_) => recover_dependency_pair(manifest_path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

/// Recover before project reads; never overwrite bytes outside the recorded original/staged pair.
pub fn recover_dependency_pair(manifest_path: &Path) -> Result<bool, ProjectError> {
    let guard = Guard::acquire(manifest_path)?;
    recover(&guard)
}

fn recover(guard: &Guard) -> Result<bool, ProjectError> {
    if !guard.journal.try_exists().map_err(io_error)? {
        return Ok(false);
    }
    check_file(&guard.journal, false)?;
    let mut bytes = Vec::new();
    File::open(&guard.journal).map_err(io_error)?.take(JOURNAL_LIMIT + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(recovery_guidance(guard, "journal exceeds the recovery size limit"));
    }
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|error| recovery_guidance(guard, format!("invalid journal: {error}")))?;
    if journal.schema_version != 1
        || journal.project_directory != guard.directory
        || Some(journal.manifest_name.as_str()) != guard.manifest.file_name().and_then(|name| name.to_str())
    {
        return Err(recovery_guidance(guard, "journal project identity/version mismatch"));
    }
    if journal.state == CommitState::Committed {
        expected(&guard.manifest, Some(&journal.replacement_manifest))
            .map_err(|error| recovery_guidance(guard, error))?;
        expected(&guard.lock, Some(&journal.replacement_lock)).map_err(|error| recovery_guidance(guard, error))?;
        clear_journal(guard)?;
    } else {
        restore(guard, &journal).map_err(|error| recovery_guidance(guard, error))?;
    }
    Ok(true)
}

fn restore(guard: &Guard, journal: &Journal) -> Result<(), ProjectError> {
    // Check BOTH before changing either; an external editor owns any unrecognized bytes.
    recoverable(
        &guard.manifest,
        Some(&journal.original_manifest),
        &journal.replacement_manifest,
        &journal.replacement_manifest_identity,
    )?;
    recoverable(
        &guard.lock,
        journal.original_lock.as_deref(),
        &journal.replacement_lock,
        &journal.replacement_lock_identity,
    )?;
    restore_one(
        guard,
        &guard.manifest,
        Some(&journal.original_manifest),
        &journal.replacement_manifest,
        &journal.replacement_manifest_identity,
    )?;
    restore_one(
        guard,
        &guard.lock,
        journal.original_lock.as_deref(),
        &journal.replacement_lock,
        &journal.replacement_lock_identity,
    )?;
    clear_journal(guard)
}
fn restore_one(
    guard: &Guard,
    path: &Path,
    original: Option<&[u8]>,
    replacement: &[u8],
    identity: &FileIdentity,
) -> Result<(), ProjectError> {
    recoverable(path, original, replacement, identity)?;
    if read_optional(path)?.as_deref() == original {
        return Ok(());
    }
    match original {
        Some(bytes) => {
            let stage = stage_published(&guard.private, path, bytes, &NativeDependencyTransactionIo)?;
            recoverable(path, original, replacement, identity)?;
            NativeDependencyTransactionIo.replace(&stage, path).map_err(io_error)?;
        }
        None => {
            recoverable(path, original, replacement, identity)?;
            fs::remove_file(path).map_err(io_error)?;
        }
    }
    NativeDependencyTransactionIo.sync_directory(&guard.directory).map_err(io_error)
}
fn recoverable(
    path: &Path,
    original: Option<&[u8]>,
    replacement: &[u8],
    identity: &FileIdentity,
) -> Result<(), ProjectError> {
    let current = read_optional(path)?;
    if current.as_deref() == original {
        return Ok(());
    }
    if current.as_deref() == Some(replacement) && FileIdentity::read(path).map_err(io_error)? == *identity {
        return Ok(());
    }
    Err(failure(format!("external bytes or file identity changed at {}; refusing recovery overwrite", path.display())))
}
fn expected(path: &Path, bytes: Option<&[u8]>) -> Result<(), ProjectError> {
    if read_optional(path)?.as_deref() != bytes {
        return Err(failure(format!(
            "concurrent dependency edit at {}; planned bytes no longer match",
            path.display()
        )));
    }
    Ok(())
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ProjectError> {
    check_file(path, true)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}
fn stage(directory: &Path, bytes: &[u8], io: &dyn DependencyTransactionIo) -> Result<TempPath, ProjectError> {
    stage_file(directory, bytes, StageAccess::Private, io)
}
/// Access of a staged file once it is renamed into place.
enum StageAccess {
    /// Private transaction state (owner-only on Unix).
    Private,
    /// A public file with no destination yet: ordinary creation (Unix 0666 filtered by the umask).
    NewPublic,
    /// A public file that replaces a destination and keeps its Unix permissions.
    #[cfg(unix)]
    Existing(fs::Permissions),
}
// Public files retain destination permissions. New Unix files use ordinary
// 0666 creation filtered by the process umask, without changing that umask.
// On Windows a published file takes the ordinary attributes and inherited ACL of a new project file:
// `Permissions` there is the whole attribute set, so copying it would also copy attributes such as
// FILE_ATTRIBUTE_TEMPORARY from the destination.
fn stage_published(
    directory: &Path,
    destination: &Path,
    bytes: &[u8],
    io: &dyn DependencyTransactionIo,
) -> Result<TempPath, ProjectError> {
    let access = match fs::metadata(destination) {
        #[cfg(unix)]
        Ok(metadata) => StageAccess::Existing(metadata.permissions()),
        #[cfg(not(unix))]
        Ok(_) => StageAccess::NewPublic,
        Err(error) if error.kind() == io::ErrorKind::NotFound => StageAccess::NewPublic,
        Err(error) => return Err(io_error(error)),
    };
    stage_file(directory, bytes, access, io)
}
/// Write and durably sync a staged file, then close its handle.
///
/// The stage is created with ordinary open options rather than `tempfile`'s default creator: on
/// Windows that creator marks files `FILE_ATTRIBUTE_TEMPORARY`, and a rename keeps the attribute, so
/// the published manifest and lock would stay marked temporary. The returned `TempPath` holds no
/// open handle, so the rename never runs while this process holds a handle to the staged file (or,
/// after the rename, to the destination), and an unpublished stage is still removed on drop.
fn stage_file(
    directory: &Path,
    bytes: &[u8],
    access: StageAccess,
    io: &dyn DependencyTransactionIo,
) -> Result<TempPath, ProjectError> {
    #[cfg(not(unix))]
    let _ = access;
    let mut file = tempfile::Builder::new()
        .prefix("stage-")
        .make_in(directory, |path| {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(if matches!(access, StageAccess::NewPublic) { 0o666 } else { 0o600 });
            }
            options.open(path)
        })
        .map_err(io_error)?;
    #[cfg(unix)]
    if let StageAccess::Existing(permissions) = access {
        file.as_file().set_permissions(permissions).map_err(io_error)?;
    }
    file.write_all(bytes).map_err(io_error)?;
    io.sync_file(file.as_file()).map_err(io_error)?;
    Ok(file.into_temp_path())
}
fn save_journal(guard: &Guard, journal: &Journal, io: &dyn DependencyTransactionIo) -> Result<(), ProjectError> {
    let bytes =
        serde_json::to_vec(journal).map_err(|error| failure(format!("journal serialization failed: {error}")))?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(failure("transaction journal exceeds size limit"));
    }
    let stage = stage(&guard.private, &bytes, io)?;
    check_file(&guard.journal, true)?;
    io.replace(&stage, &guard.journal).map_err(io_error)?;
    io.sync_directory(&guard.private).map_err(io_error)?;
    // Ensure discovery of the journal directory is also durable on first use.
    io.sync_directory(guard.private.parent().unwrap()).map_err(io_error)?;
    io.sync_directory(&guard.directory).map_err(io_error)
}
fn clear_journal(guard: &Guard) -> Result<(), ProjectError> {
    fs::remove_file(&guard.journal).map_err(io_error)?;
    NativeDependencyTransactionIo.sync_directory(&guard.private).map_err(io_error)
}
fn reject_symlink(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() {
        return Err(failure(format!("symlink transaction destination rejected: {}", path.display())));
    }
    Ok(())
}
fn check_file(path: &Path, missing_allowed: bool) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(failure(format!(
                    "non-regular or symlink transaction destination rejected: {}",
                    path.display()
                )));
            }
            check_owner(path, &metadata)?;
            Ok(())
        }
        Err(error) if missing_allowed && error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
fn secure_directory(path: &Path) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(failure(format!("unsafe transaction directory: {}", path.display())));
            }
            check_owner(path, &metadata)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if path.file_name().is_some_and(|name| name == "dependency-transaction") && metadata.mode() & 0o077 != 0
                {
                    return Err(failure(format!("transaction directory must be private (0700): {}", path.display())));
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
            builder.create(path).map_err(io_error)?;
        }
        Err(error) => return Err(io_error(error)),
    }
    Ok(())
}
fn check_owner(path: &Path, metadata: &fs::Metadata) -> Result<(), ProjectError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Creator-owned anonymous file supplies the effective filesystem UID without unsafe FFI.
        let creator = tempfile::tempfile().map_err(io_error)?.metadata().map_err(io_error)?.uid();
        if metadata.uid() != creator {
            return Err(failure(format!("foreign-owned transaction destination rejected: {}", path.display())));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, metadata);
    }
    Ok(())
}
fn failure(message: impl std::fmt::Display) -> ProjectError {
    ProjectError::Validation(message.to_string())
}
fn io_error(error: io::Error) -> ProjectError {
    failure(format!("dependency transaction I/O failure: {error}"))
}
fn recovery_guidance(guard: &Guard, message: impl std::fmt::Display) -> ProjectError {
    failure(format!(
        "dependency transaction recovery required at {}: {message}; preserve journal and both files, resolve external edits, then rerun recovery before project reads",
        guard.journal.display()
    ))
}
