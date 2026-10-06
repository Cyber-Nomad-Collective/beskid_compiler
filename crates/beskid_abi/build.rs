mod compiler_driver_embed;
#[path = "corelib_workspace_source.rs"]
mod corelib_workspace_source;
mod sdk_source_embed;

fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = beskid_manifest::default_manifest_path(manifest_dir);
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    compiler_driver_embed::emit(manifest_dir, &out_dir);
    println!("cargo:rerun-if-changed=compiler_driver_embed.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=corelib_workspace_source.rs");

    println!("cargo:rerun-if-env-changed=BESKID_CORELIB_SOURCE");
    let corelib_override = std::env::var_os("BESKID_CORELIB_SOURCE");
    let corelib_workspace =
        corelib_workspace_source::resolve_corelib_workspace(manifest_dir, corelib_override.as_deref()).expect(
            "compiler/corelib workspace is missing or invalid; initialize the `corelib` submodule or set \
             BESKID_CORELIB_SOURCE to a valid workspace with a .bws manifest and beskid_corelib/",
        );
    for path in corelib_workspace_source::workspace_validation_files(&corelib_workspace, corelib_override.as_deref())
        .expect("read Corelib workspace validation inputs")
    {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    sdk_source_embed::emit(&corelib_workspace.join("packages/compiler-sdk"), &out_dir);
    println!("cargo:rerun-if-changed=sdk_source_embed.rs");

    let source = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("beskid_abi build: read runtime manifest: {err}"));
    let manifest = beskid_manifest::load_v5_manifest_source(&source)
        .unwrap_or_else(|err| panic!("beskid_abi build: parse ABI-v5 runtime manifest: {err}"));
    let artifacts = beskid_manifest::generate_v5_artifacts(&manifest)
        .unwrap_or_else(|err| panic!("beskid_abi build: generate ABI-v5 artifacts: {err}"));
    for (name, contents) in [
        ("abi_v5_contract.rs", artifacts.rust),
        ("beskid_runtime_abi_v5.h", artifacts.c_header),
        ("abi-v5.json", artifacts.abi_json),
        ("abi-v5-audit.json", artifacts.audit_json),
    ] {
        std::fs::write(out_dir.join(name), contents)
            .unwrap_or_else(|err| panic!("beskid_abi build: write generated `{name}`: {err}"));
    }
    for (target, contents) in artifacts.gnu_asm.into_iter().chain(artifacts.masm) {
        let name = format!("beskid_runtime_abi_v5_{}.inc", target.replace('-', "_"));
        std::fs::write(out_dir.join(&name), contents)
            .unwrap_or_else(|err| panic!("beskid_abi build: write generated `{name}`: {err}"));
    }
}
