//! Verified Beskid release-manifest parsing and direct-install management.

mod commands;
mod install;
mod manifest;
mod owner;

pub use commands::{UpArgs, UpCommand, execute};

/// Version shown by `beskid-up --version`: the release version the release build set in
/// `BESKID_RELEASE_VERSION` (the same variable stamps `beskid` and `beskid_lsp`), else the manifest
/// version. Read at build time.
pub const VERSION: &str = match option_env!("BESKID_RELEASE_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};
pub use install::{DirectInstall, update_configured_toolchain};
pub use manifest::{Bundle, ReleaseManifest, UpError};
pub use owner::{
    InstallationOwner, InstallationStatus, current_installation_status, inspect_installation, write_owner_receipt,
};
