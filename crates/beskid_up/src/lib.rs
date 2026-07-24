//! Verified Beskid release-manifest parsing and direct-install management.

mod commands;
mod install;
mod manifest;

pub use commands::{UpArgs, UpCommand, execute};
pub use install::DirectInstall;
pub use manifest::{Bundle, ReleaseManifest, UpError};
