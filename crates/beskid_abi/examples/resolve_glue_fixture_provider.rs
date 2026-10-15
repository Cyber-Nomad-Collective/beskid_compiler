//! Source-bound resolver for native fixture recipes, not a release qualification issuer.
use beskid_abi::runtime_kit::{BuildProfile, host_runtime_target, resolve_glue_shared_provider};
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let prefix = std::path::PathBuf::from(args.next().ok_or("expected one installed runtime prefix")?);
    if args.next().is_some() { return Err("expected one installed runtime prefix".into()); }
    let target = host_runtime_target()?;
    let provider = resolve_glue_shared_provider(&prefix, &target, BuildProfile::Debug)?;
    let header = include_bytes!("../include/beskid_runtime_abi_v5.h");
    println!("{}", serde_json::to_string(&serde_json::json!({
        "schema_version":1, "scope":"canonical-provider-fixture-inputs",
        "target": target.triple.as_str(), "profile":"debug",
        "kit_root":provider.kit.root, "shared_library":provider.shared_library,
        "link_library":provider.link_library, "manifest":provider.manifest,
        "abi_header_sha256":format!("{:x}",Sha256::digest(header)),
        "runtime_sources_current":true, "release_qualified":false
    }))?);
    Ok(())
}
