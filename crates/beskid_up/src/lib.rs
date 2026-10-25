//! Verified Beskid release-manifest parsing and direct-install management.

mod commands;
mod install;
mod manifest;
mod owner;

pub use commands::{UpArgs, UpCommand, execute};
pub use install::{DirectInstall, update_configured_toolchain};
pub use manifest::{Bundle, ReleaseManifest, UpError};
pub use owner::{
    InstallationOwner, InstallationStatus, current_installation_status, inspect_installation, write_owner_receipt,
};
