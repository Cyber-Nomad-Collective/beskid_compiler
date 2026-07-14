//! Verified Beskid release-manifest parsing and direct-install management.

mod manifest;
mod install;
mod commands;

pub use commands::{execute, UpArgs, UpCommand};
pub use install::DirectInstall;
pub use manifest::{Bundle, ReleaseManifest, UpError};
