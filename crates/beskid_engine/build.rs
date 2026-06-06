fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let generated_dir = manifest_dir.join("src/generated");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    beskid_manifest::generate_jit_registration_from_path(&manifest_path, &generated_dir)
        .unwrap_or_else(|err| panic!("beskid_engine build: generate kernel registration: {err}"));
}
