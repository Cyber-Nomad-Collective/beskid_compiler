fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let builtins_path = manifest_dir.join("src/generated/builtins.inc.rs");
    let handlers_path = manifest_dir.join("src/generated/runtime_handlers.inc.rs");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    beskid_manifest::generate_analysis_from_path(&manifest_path, &builtins_path)
        .unwrap_or_else(|err| panic!("beskid_analysis build: generate builtins: {err}"));
    beskid_manifest::generate_runtime_handlers_from_path(&manifest_path, &handlers_path)
        .unwrap_or_else(|err| panic!("beskid_analysis build: generate runtime handlers: {err}"));
}
