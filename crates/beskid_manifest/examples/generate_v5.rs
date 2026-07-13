use std::path::PathBuf;

fn main() -> Result<(), String> {
    let workspace = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: cargo run -p beskid_manifest --example generate_v5 -- <workspace>")?;
    let source = std::fs::read_to_string(workspace.join("runtime_manifest.bsol"))
        .map_err(|error| error.to_string())?;
    let manifest = beskid_manifest::load_v5_manifest_source(&source)?;
    beskid_manifest::write_v5_artifacts(&manifest, &workspace)
}
