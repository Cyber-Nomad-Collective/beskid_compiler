//! Trusted compiler build input, never a runtime sidecar or user qualification DTO.
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn emit(manifest: &Path, output: &Path) {
    println!("cargo:rerun-if-env-changed=BESKID_COMPILER_DRIVER_BUILD_RECEIPT");
    let root = manifest.join("../beskid_execution");
    let mut source = Sha256::new();
    source.update(b"beskid.compiler-driver.source/1");
    for name in ["Cargo.toml", "src/lib.rs", "src/compiler_driver.rs", "src/bin/beskid_native_tool_driver.rs"] {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).expect("compiler driver owned source");
        source.update((name.len() as u64).to_le_bytes());
        source.update(name.as_bytes());
        source.update((bytes.len() as u64).to_le_bytes());
        source.update(bytes);
    }
    let expected_source = format!("{:x}", source.finalize());
    let proof = match std::env::var_os("BESKID_COMPILER_DRIVER_BUILD_RECEIPT") {
        None => "None".to_owned(),
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = fs::read(&path).expect("owned driver build receipt");
            assert!(bytes.len() <= 65536, "driver receipt bound");
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("driver receipt JSON");
            let keys = value.as_object().expect("driver receipt object");
            assert_eq!(keys.len(), 6, "closed driver receipt");
            assert_eq!(value["version"].as_u64(), Some(1));
            assert_eq!(value["source_sha256"].as_str(), Some(expected_source.as_str()), "driver source differs");
            let target = std::env::var("TARGET").expect("TARGET");
            assert_eq!(value["target"].as_str(), Some(target.as_str()), "driver target differs");
            assert_eq!(value["package"].as_str(), Some("beskid_execution"));
            let payload = Path::new(value["executable"].as_str().expect("driver payload"));
            assert_eq!(fs::canonicalize(payload).expect("driver canonical payload"), payload);
            let metadata = fs::symlink_metadata(payload).expect("driver metadata");
            assert!(metadata.is_file() && metadata.len() <= 128 * 1024 * 1024, "driver regular bound");
            println!("cargo:rerun-if-changed={}", payload.display());
            let actual = format!("{:x}", Sha256::digest(fs::read(payload).expect("driver bytes")));
            assert_eq!(value["sha256"].as_str(), Some(actual.as_str()), "driver bytes differ");
            format!("Some(({target:?},{actual:?},{expected_source:?}))")
        }
    };
    fs::write(
        output.join("compiler_driver_identity.rs"),
        format!("pub const QUALIFIED_COMPILER_DRIVER:Option<(&str,&str,&str)>={proof};\n"),
    )
    .expect("driver embedded identity");
}
