fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let generated_dir = manifest_dir.join("src/generated");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let rust_fallback_handlers = std::env::var("CARGO_FEATURE_RUST_FALLBACK_HANDLERS").is_ok();
    beskid_manifest::generate_runtime_from_path(
        &manifest_path,
        &generated_dir,
        rust_fallback_handlers,
    )
    .unwrap_or_else(|err| panic!("beskid_runtime build: generate dispatch table: {err}"));
}
