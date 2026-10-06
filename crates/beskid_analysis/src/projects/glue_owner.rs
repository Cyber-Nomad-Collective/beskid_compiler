//! Closed source inventory of a manifest `glue "<library>" { backend = rust path = "<dir>" }` owner.
//!
//! The manifest validator checks the owner declaration lexically. This module resolves the owner
//! directory against the project root on disk, confines it after canonicalization, and collects
//! its Rust sources. The directory content is closed: it must contain `implementation.rs` and only
//! regular `.rs` files (in nested directories as well), within the producer bounds. Every other
//! entry is rejected, never ignored, and a failure returns no partial inventory.
//!
//! Diagnostic codes (E1801-E1899 manifest band):
//! - **E1862**: owner backend unavailable (`dotnet` in 0.6).
//! - **E1863**: owner directory is missing, not a directory, or not confined to the project root.
//! - **E1865**: owner directory content is not closed (missing `implementation.rs`, a reserved or
//!   non-`.rs` file, a symbolic link, a non-regular entry, a non-UTF-8 name or source).
//! - **E1866**: producer bounds exceeded.

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::projects::{
    error::ProjectError,
    model::{ProjectGlueBackend, ProjectGlueOwner},
    validator::validate_glue_owner_path,
};

/// Entry source file every Rust Glue owner directory must contain at its top level.
pub const GLUE_OWNER_ENTRY_FILE: &str = "implementation.rs";
/// Maximum number of source files in one owner directory.
pub const GLUE_OWNER_MAX_FILES: usize = 1024;
/// Maximum number of directories (including the owner directory) walked in one owner directory.
pub const GLUE_OWNER_MAX_DIRECTORIES: usize = 1024;
/// Maximum size of one owner source file in bytes (8 MiB).
pub const GLUE_OWNER_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum total size of all owner source files in bytes (32 MiB).
pub const GLUE_OWNER_MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;

/// File names the compiler generates for the owner package; a user copy is rejected.
const GLUE_OWNER_RESERVED_FILES: &[&str] = &["Cargo.toml", "Cargo.lock", "build.rs", "owner_bridge.rs"];

/// One collected owner source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlueOwnerSourceFile {
    /// Path relative to the owner directory, `/`-separated, with only normal components.
    pub relative_path: String,
    /// Exact UTF-8 file bytes.
    pub bytes: Vec<u8>,
}

/// Complete, closed source inventory of one Rust Glue owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlueOwnerSources {
    /// Canonical owner native library identity (the manifest `glue` label).
    pub library: String,
    /// Canonical owner source directory, confined to the canonical project root.
    pub directory: PathBuf,
    /// Source files sorted by `relative_path`; always contains [`GLUE_OWNER_ENTRY_FILE`].
    pub files: Vec<GlueOwnerSourceFile>,
}

/// Resolves the canonical owner source directory and checks that it is confined to the project root
/// and reached without symbolic links.
pub fn glue_owner_directory(project_root: &Path, owner: &ProjectGlueOwner) -> Result<PathBuf, ProjectError> {
    validate_glue_owner_path(owner)?;
    let canonical_root = fs::canonicalize(project_root).map_err(|error| {
        directory_error(owner, format!("project root `{}` cannot be resolved: {error}", project_root.display()))
    })?;
    let mut expected = canonical_root.clone();
    for component in Path::new(&owner.path).components() {
        match component {
            Component::Normal(part) => expected.push(part),
            Component::CurDir => {}
            _ => return Err(directory_error(owner, "must be a relative path inside the project root".to_string())),
        }
    }
    let canonical = fs::canonicalize(project_root.join(&owner.path))
        .map_err(|error| directory_error(owner, format!("cannot be resolved: {error}")))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(directory_error(owner, "resolves outside the project root".to_string()));
    }
    if canonical != expected {
        return Err(directory_error(owner, "is reached through a symbolic link".to_string()));
    }
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|error| directory_error(owner, format!("cannot be inspected: {error}")))?;
    if !metadata.is_dir() {
        return Err(directory_error(owner, "is not a directory".to_string()));
    }
    Ok(canonical)
}

/// Collects the closed Rust source inventory of one validated `glue` owner.
///
/// `project_root` is the directory that contains the project manifest. The owner declaration must
/// come from a manifest accepted by [`super::validator::validate_manifest`]; this function re-checks
/// the backend and the lexical path so that it cannot admit an unvalidated owner.
pub fn collect_glue_owner_sources(
    project_root: &Path,
    owner: &ProjectGlueOwner,
) -> Result<GlueOwnerSources, ProjectError> {
    if owner.backend != ProjectGlueBackend::Rust {
        return Err(ProjectError::meta_contract(
            "E1862",
            format!(
                "glue `{}` selects backend `{}`, which is unavailable in this release",
                owner.library,
                owner.backend.as_str()
            ),
        ));
    }
    let directory = glue_owner_directory(project_root, owner)?;
    let mut collector = Collector { owner, files: Vec::new(), directories: 0, total_bytes: 0 };
    collector.walk(&directory, "")?;
    let mut files = collector.files;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if !files.iter().any(|file| file.relative_path == GLUE_OWNER_ENTRY_FILE) {
        return Err(content_error(owner, GLUE_OWNER_ENTRY_FILE, "is missing; it is the required owner entry file"));
    }
    Ok(GlueOwnerSources { library: owner.library.clone(), directory, files })
}

struct Collector<'a> {
    owner: &'a ProjectGlueOwner,
    files: Vec<GlueOwnerSourceFile>,
    directories: usize,
    total_bytes: u64,
}

impl Collector<'_> {
    fn walk(&mut self, directory: &Path, relative_prefix: &str) -> Result<(), ProjectError> {
        self.directories += 1;
        if self.directories > GLUE_OWNER_MAX_DIRECTORIES {
            return Err(bound_error(
                self.owner,
                relative_prefix,
                format!("exceeds the limit of {GLUE_OWNER_MAX_DIRECTORIES} directories"),
            ));
        }
        let display_dir = if relative_prefix.is_empty() { "." } else { relative_prefix };
        let reader = fs::read_dir(directory)
            .map_err(|error| content_error(self.owner, display_dir, &format!("cannot be listed: {error}")))?;
        let mut entries = Vec::new();
        for entry in reader {
            let entry = entry
                .map_err(|error| content_error(self.owner, display_dir, &format!("cannot be listed: {error}")))?;
            entries.push(entry);
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let raw_name = entry.file_name();
            let Some(name) = raw_name.to_str() else {
                let lossy = join_relative(relative_prefix, &raw_name.to_string_lossy());
                return Err(content_error(self.owner, &lossy, "has a non-UTF-8 name"));
            };
            let relative = join_relative(relative_prefix, name);
            if name.contains('\\') {
                return Err(content_error(self.owner, &relative, "has a `\\` in its name"));
            }
            let file_type = entry
                .file_type()
                .map_err(|error| content_error(self.owner, &relative, &format!("cannot be inspected: {error}")))?;
            if file_type.is_symlink() {
                return Err(content_error(self.owner, &relative, "is a symbolic link"));
            }
            if file_type.is_dir() {
                self.walk(&entry.path(), &relative)?;
                continue;
            }
            if !file_type.is_file() {
                return Err(content_error(self.owner, &relative, "is not a regular file"));
            }
            if GLUE_OWNER_RESERVED_FILES.contains(&name) {
                return Err(content_error(
                    self.owner,
                    &relative,
                    "is reserved; the compiler generates the owner Cargo package, lock and bridge",
                ));
            }
            let stem_is_empty = name.strip_suffix(".rs").is_none_or(str::is_empty);
            if stem_is_empty {
                return Err(content_error(self.owner, &relative, "is not a `.rs` source file"));
            }
            self.collect_file(&entry.path(), relative)?;
        }
        Ok(())
    }

    fn collect_file(&mut self, path: &Path, relative: String) -> Result<(), ProjectError> {
        if self.files.len() >= GLUE_OWNER_MAX_FILES {
            return Err(bound_error(
                self.owner,
                &relative,
                format!("exceeds the limit of {GLUE_OWNER_MAX_FILES} source files"),
            ));
        }
        let file = fs::File::open(path)
            .map_err(|error| content_error(self.owner, &relative, &format!("cannot be read: {error}")))?;
        let mut bytes = Vec::new();
        file.take(GLUE_OWNER_MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| content_error(self.owner, &relative, &format!("cannot be read: {error}")))?;
        let len = bytes.len() as u64;
        if len > GLUE_OWNER_MAX_FILE_BYTES {
            return Err(bound_error(
                self.owner,
                &relative,
                format!("exceeds the limit of {GLUE_OWNER_MAX_FILE_BYTES} bytes per source file"),
            ));
        }
        self.total_bytes += len;
        if self.total_bytes > GLUE_OWNER_MAX_TOTAL_BYTES {
            return Err(bound_error(
                self.owner,
                &relative,
                format!("exceeds the limit of {GLUE_OWNER_MAX_TOTAL_BYTES} total source bytes"),
            ));
        }
        if std::str::from_utf8(&bytes).is_err() {
            return Err(content_error(self.owner, &relative, "is not valid UTF-8"));
        }
        self.files.push(GlueOwnerSourceFile { relative_path: relative, bytes });
        Ok(())
    }
}

fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") }
}

fn directory_error(owner: &ProjectGlueOwner, reason: String) -> ProjectError {
    ProjectError::meta_contract("E1863", format!("glue `{}` path `{}` {reason}", owner.library, owner.path))
}

fn content_error(owner: &ProjectGlueOwner, relative: &str, reason: &str) -> ProjectError {
    ProjectError::meta_contract(
        "E1865",
        format!("glue `{}` source `{}/{relative}` {reason}", owner.library, owner.path.trim_end_matches('/')),
    )
}

fn bound_error(owner: &ProjectGlueOwner, relative: &str, reason: String) -> ProjectError {
    ProjectError::meta_contract(
        "E1866",
        format!("glue `{}` source `{}/{relative}` {reason}", owner.library, owner.path.trim_end_matches('/')),
    )
}

#[cfg(test)]
mod tests;
