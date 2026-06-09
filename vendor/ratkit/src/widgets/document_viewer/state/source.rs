//! Source loading state shared by document viewers.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Stores document content and optional file metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceState {
    content: String,
    source_path: Option<PathBuf>,
}

impl SourceState {
    /// Replaces content with an in-memory string.
    pub fn set_source_string(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.source_path = None;
    }

    /// Loads content from a file path.
    pub fn set_source_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        self.content = fs::read_to_string(path)?;
        self.source_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Returns the current content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns the source path when content came from a file.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
