fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let generated_dir = manifest_dir.join("src/generated");
    beskid_manifest::generate_host_from_path(&manifest_path, &generated_dir)
        .expect("generate beskid_host handler table");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
}
