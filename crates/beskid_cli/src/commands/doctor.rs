//! Read-only installation observations.
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct DoctorArgs {}

pub fn execute(_: DoctorArgs) -> Result<()> {
    let target = beskid_abi::runtime_kit::host_runtime_target()?;
    println!("host: {}", target.triple.as_str());
    let prefix = match beskid_abi::runtime_kit::installed_toolchain_prefix() {
        Ok(prefix) => prefix,
        Err(error) => {
            anyhow::bail!(
                "doctor: installation prefix unavailable: {error}; install a complete toolchain bundle"
            )
        }
    };
    for profile in [
        beskid_abi::runtime_kit::BuildProfile::Debug,
        beskid_abi::runtime_kit::BuildProfile::Release,
    ] {
        beskid_abi::runtime_kit::resolve_installed_runtime_kit(&prefix, &target, profile).map_err(|error| {
            anyhow::anyhow!("doctor: {profile:?} runtime kit invalid: {error:?}; run `beskid toolchain update`")
        })?;
        println!("runtime {profile:?}: verified");
    }
    let corelib = prefix.join("beskid_corelib");
    if !beskid_abi::corelib_bundle::verified_corelib_bundle_root(&corelib)
        .is_some_and(|verified| corelib.canonicalize().ok().as_ref() == Some(&verified))
    {
        anyhow::bail!("doctor: Corelib bundle missing or invalid; run `beskid toolchain update`");
    }
    println!("Corelib: verified");
    Ok(())
}
