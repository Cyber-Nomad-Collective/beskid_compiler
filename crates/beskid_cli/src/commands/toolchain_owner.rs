//! Prefix-bound receipt production for qualified local installer staging.
use anyhow::Result;
use clap::{Args, ValueEnum};
use std::path::Path;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PackageOwner {
    Homebrew,
    Debian,
    MacosInstaller,
    WindowsInstaller,
    Manual,
    Container,
}
impl From<PackageOwner> for beskid_up::InstallationOwner {
    fn from(owner: PackageOwner) -> Self {
        use beskid_up::InstallationOwner as Owner;
        match owner {
            PackageOwner::Homebrew => Owner::Homebrew,
            PackageOwner::Debian => Owner::Debian,
            PackageOwner::MacosInstaller => Owner::MacosInstaller,
            PackageOwner::WindowsInstaller => Owner::WindowsInstaller,
            PackageOwner::Manual => Owner::Manual,
            PackageOwner::Container => Owner::Container,
        }
    }
}
#[derive(Args, Debug)]
pub struct ToolchainOwnerArgs {
    #[arg(long, value_enum)]
    pub owner: PackageOwner,
    #[arg(long)]
    pub version: semver::Version,
    #[arg(long)]
    pub target: String,
}
pub fn execute(args: ToolchainOwnerArgs) -> Result<()> {
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    let prefix = beskid_abi::runtime_kit::installed_toolchain_prefix_for_executable(Path::new(&executable))?;
    beskid_up::WriteOwnerReceipt(&prefix, args.owner.into(), &args.version, &args.target)?;
    println!(
        "Verified {} installation owner at {}",
        beskid_up::InstallationOwner::from(args.owner).name(),
        prefix.display()
    );
    Ok(())
}
