fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let source = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("beskid_runtime build: read ABI-v5 manifest: {err}"));
    beskid_manifest::load_v5_manifest_source(&source)
        .unwrap_or_else(|err| panic!("beskid_runtime build: validate ABI-v5 manifest: {err}"));
}
