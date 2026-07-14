use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeKitBuildError, RuntimeKitBuildRequest, build_runtime_kit,
    resolve_installed_runtime_kit,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "beskid-runtime-kit-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn linux_target() -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap()
}

fn request(prefix: &TempDir, inputs: &TempDir) -> RuntimeKitBuildRequest {
    let static_library = inputs.0.join("libbeskid_runtime.a");
    let shared_library = inputs.0.join("libbeskid_runtime.so");
    fs::write(&static_library, b"canonical static runtime").unwrap();
    fs::write(&shared_library, b"canonical shared runtime").unwrap();
    RuntimeKitBuildRequest {
        prefix: prefix.0.clone(),
        target: linux_target(),
        profile: BuildProfile::Release,
        runtime_source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .into(),
        static_library,
        shared_library,
        shared_import_library: None,
    }
}

#[test]
fn builds_a_resolvable_kit_from_prebuilt_artifacts_and_writes_metadata_last() {
    let prefix = TempDir::new("prefix");
    let inputs = TempDir::new("inputs");
    let request = request(&prefix, &inputs);

    let built = build_runtime_kit(&request).unwrap();
    assert_eq!(
        fs::read(&built.static_library).unwrap(),
        b"canonical static runtime"
    );
    assert_eq!(
        fs::read(&built.shared_library).unwrap(),
        b"canonical shared runtime"
    );
    assert!(built.root.join("abi.json").is_file());

    let resolved =
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Release).unwrap();
    assert_eq!(resolved.metadata, built.metadata);
}

#[test]
fn failed_input_validation_publishes_no_partial_kit_or_metadata() {
    let prefix = TempDir::new("prefix-failure");
    let inputs = TempDir::new("inputs-failure");
    let mut request = request(&prefix, &inputs);
    request.shared_library = inputs.0.join("missing-runtime.so");

    assert!(matches!(
        build_runtime_kit(&request),
        Err(RuntimeKitBuildError::SourceArtifactRead { .. })
    ));
    let expected_root = prefix
        .0
        .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/release");
    assert!(!expected_root.exists());
    assert!(!expected_root.join("abi.json").exists());
}
