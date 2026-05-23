//! Normative `ExternalLibrary` provider trait.

use std::path::PathBuf;

use super::error::LibraryResolveError;

/// Resolves a logical library name (as it appears in `Extern(..., Library: "...")`) to platform
/// linker arguments and search paths.
///
/// The trait matches the normative shape published in
/// `site/website/src/content/docs/platform-spec/tooling/foreign-library-import/external-library-trait.mdx`.
pub trait ExternalLibrary: Send + Sync {
    /// Stable provider id, for example `"c-posix"`.
    fn id(&self) -> &'static str;

    /// Target triple or host key this provider supports (for example `"linux"`, `"macos"`,
    /// `"posix"`). The CLI normalizes the runtime host through `current_host_key()`.
    fn host_key(&self) -> &str;

    /// Map a logical library name to linker arguments (for example `["-lc"]`).
    fn resolve_link_args(&self, logical: &str) -> Result<Vec<String>, LibraryResolveError>;

    /// Optional search paths to pass the linker when the logical name is a path or sandbox SDK.
    ///
    /// Default returns an empty vector.
    fn resolve_search_paths(&self, _logical: &str) -> Vec<PathBuf> {
        Vec::new()
    }
}
