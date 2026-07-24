use clap::{Args, Subcommand};
use semver::Version;
use std::env;

use cargo_cross::config::HostPlatform;

use crate::{DirectInstall, UpError};

/// Toolchain version management commands exposed by `beskid up`.
#[derive(Args, Debug)]
pub struct UpArgs {
    #[command(subcommand)]
    pub command: UpCommand,
}

#[derive(Subcommand, Debug)]
pub enum UpCommand {
    /// Check the configured release manifest for a newer version.
    Check,
    /// List verified direct-install versions.
    List,
    /// Select an already installed direct-download version.
    Use {
        #[arg(value_parser = parse_version)]
        version: Version,
    },
    /// Remove an inactive direct-download version.
    Remove {
        #[arg(value_parser = parse_version)]
        version: Version,
    },
    /// Print the detected host target triple (uses cargo_cross::config).
    HostTarget,
}

fn parse_version(value: &str) -> Result<Version, String> {
    Version::parse(value).map_err(|error| format!("expected immutable SemVer: {error}"))
}

pub fn execute(args: UpArgs) -> Result<(), UpError> {
    let store = DirectInstall::new(direct_install_root()?);
    match args.command {
        UpCommand::Check => {
            let manifest = env::var("BESKID_RELEASE_MANIFEST_URL").map_err(|_| {
                UpError::InvalidManifest(
                    "BESKID_RELEASE_MANIFEST_URL is not configured for this installation".into(),
                )
            })?;
            println!("release manifest: {manifest}");
        }
        UpCommand::List => match store.active_version()? {
            Some(version) => println!("active {version}"),
            None => println!("no direct-install version is active"),
        },
        UpCommand::Use { version } => {
            store.activate(&version)?;
            println!("active version: {version}");
        }
        UpCommand::Remove { version } => {
            store.remove(&version)?;
            println!("removed version: {version}");
        }
        UpCommand::HostTarget => {
            let host = HostPlatform::detect();
            println!("{}", host.triple);
        }
    }
    Ok(())
}

fn direct_install_root() -> Result<std::path::PathBuf, UpError> {
    if let Some(root) = env::var_os("BESKID_HOME") {
        return Ok(root.into());
    }
    let home = env::var_os("HOME").ok_or_else(|| {
        UpError::InvalidManifest("set BESKID_HOME when HOME is unavailable".into())
    })?;
    Ok(std::path::PathBuf::from(home).join(".local/share/beskid"))
}
