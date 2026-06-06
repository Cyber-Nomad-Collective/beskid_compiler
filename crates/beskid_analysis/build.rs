fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let out_path = manifest_dir.join("src/generated/builtins.inc.rs");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    beskid_manifest::generate_analysis_from_path(&manifest_path, &out_path)
        .unwrap_or_else(|err| panic!("beskid_analysis build: generate runtime manifest: {err}"));
}
