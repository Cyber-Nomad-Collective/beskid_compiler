use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let isle_path = Path::new("isle/primitives.isle");
    let generated = Path::new(&out_dir).join("isle_generated.rs");

    let options = cranelift_isle::codegen::CodegenOptions {
        exclude_global_allow_pragmas: true,
        prefixes: vec![],
    };

    let code = cranelift_isle::compile::from_files([isle_path], &options)
        .unwrap_or_else(|err| panic!("failed to compile ISLE: {err:?}"));

    fs::write(&generated, code).expect("write generated ISLE");

    println!("cargo:rerun-if-changed=isle/primitives.isle");
    println!("cargo:rerun-if-changed=build.rs");
}
