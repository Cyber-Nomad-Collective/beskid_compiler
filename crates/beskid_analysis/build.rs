fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let transitional_builtins = manifest_dir.join("src/generated/builtins.inc.rs");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", transitional_builtins.display());
    println!("cargo:rerun-if-changed=build.rs");

    let source = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("beskid_analysis build: read ABI-v5 manifest: {err}"));
    let base = std::fs::read_to_string(&transitional_builtins)
        .unwrap_or_else(|err| panic!("beskid_analysis build: read builtin baseline: {err}"));
    beskid_manifest::generate_analysis_with_v5_intrinsics_from_source(&source, &base, &transitional_builtins)
        .unwrap_or_else(|err| panic!("beskid_analysis build: generate ABI-v5 builtin surface: {err}"));
}
