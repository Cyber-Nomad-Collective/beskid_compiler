//! Beskid Language Server: document sync, semantic features, and workspace-wide indexing.

pub(crate) mod adapters;
pub(crate) mod commands;
pub mod crash;
pub(crate) mod diagnostics;
pub(crate) mod features;
pub(crate) mod hook_items;
pub(crate) mod logging;
pub(crate) mod manifest_uri;
pub(crate) mod position;
pub(crate) mod protocol;
pub(crate) mod scan_schedule;
pub mod server;
pub(crate) mod session;
pub(crate) mod standalone_bsol;
#[cfg(test)]
pub(crate) mod test_mod_host;
pub(crate) mod text_sync;
pub(crate) mod trace;
pub(crate) mod workspace_scan;

/// Serializes unit tests that share process-global state: the working directory
/// and the process-wide entry-session / typed-entry registries keyed by the
/// `beskid_e2e_tests/fixtures/corelib_mvp` entry. Each test owns a separate
/// `State`, but concurrent prepares of the same entry race on those registries
/// ("entry session syntax generation cannot regress"), which a real server
/// avoids through the mutation gate of the project's database.
#[cfg(test)]
pub(crate) static SHARED_FIXTURE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

pub use server::stdio::serve_stdio;

/// Version shown by `beskid_lsp --version` and written into crash reports: the release version the
/// release build set in `BESKID_RELEASE_VERSION` (the same variable stamps `beskid` and `beskid-up`),
/// else the manifest version. Read at build time.
pub const VERSION: &str = match option_env!("BESKID_RELEASE_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};
