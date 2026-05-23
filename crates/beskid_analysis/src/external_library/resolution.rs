//! `LibraryResolution` — provider-agnostic resolved view returned by the registry.

use std::path::PathBuf;

/// Combined linker inputs returned after a successful `ExternalLibrary` resolution.
///
/// Used by `beskid import lib` to write the matching `link` manifest entries and to print the
/// resolved arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryResolution {
    /// Provider id that produced this resolution.
    pub provider: String,
    /// Host key reported by the provider.
    pub host_key: String,
    /// Logical name the caller asked the provider to resolve.
    pub logical: String,
    /// Concrete linker arguments (for example `["-lc"]`).
    pub link_args: Vec<String>,
    /// Optional search paths to pass the linker (`-L` flags).
    pub search_paths: Vec<PathBuf>,
}
