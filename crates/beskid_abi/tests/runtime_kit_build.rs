use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeKitBuildError, RuntimeKitBuildRequest, build_runtime_kit, resolve_installed_runtime_kit,
};
use beskid_abi::runtime_source::{
    CanonicalRuntimeKitBuildError, build_canonical_runtime_kit, canonical_runtime_source_hash,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-runtime-kit-{label}-{}-{nonce}", std::process::id()));
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
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap()
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
        runtime_source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
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
    assert_eq!(fs::read(&built.static_library).unwrap(), b"canonical static runtime");
    assert_eq!(fs::read(&built.shared_library).unwrap(), b"canonical shared runtime");
    assert!(built.root.join("abi.json").is_file());

    let resolved = resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Release).unwrap();
    assert_eq!(resolved.metadata, built.metadata);
}

#[test]
fn failed_input_validation_publishes_no_partial_kit_or_metadata() {
    let prefix = TempDir::new("prefix-failure");
    let inputs = TempDir::new("inputs-failure");
    let mut request = request(&prefix, &inputs);
    request.shared_library = inputs.0.join("missing-runtime.so");

    assert!(matches!(build_runtime_kit(&request), Err(RuntimeKitBuildError::SourceArtifactRead { .. })));
    let expected_root = prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/release");
    assert!(!expected_root.exists());
    assert!(!expected_root.join("abi.json").exists());
}

#[test]
fn canonical_runtime_kit_builder_denies_noncanonical_source_hashes() {
    let prefix = TempDir::new("canonical-source-denial");
    let inputs = TempDir::new("canonical-source-denial-inputs");
    let request = request(&prefix, &inputs);

    assert!(matches!(
        build_canonical_runtime_kit(&request),
        Err(CanonicalRuntimeKitBuildError::SourceHashMismatch { .. })
    ));

    let expected_root = prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/release");
    assert!(!expected_root.exists());
}

#[test]
fn canonical_runtime_kit_builder_publishes_the_embedded_corpus_hash() {
    let prefix = TempDir::new("canonical-source");
    let inputs = TempDir::new("canonical-source-inputs");
    let mut request = request(&prefix, &inputs);
    request.runtime_source_hash = canonical_runtime_source_hash();

    let built = build_canonical_runtime_kit(&request).expect("canonical runtime kit");
    assert_eq!(built.metadata.source_hash, canonical_runtime_source_hash());
}

#[test]
fn core_args_native_adapters_are_present_for_every_manifest_target() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(linux_target());
    for target in beskid_abi::abi_v5::TargetMetadata::supported() {
        let source = fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target.triple.as_str()).join("platform_host.c"))
            .expect("target platform host source");
        assert!(source.contains("beskid_rt_v5_args_count"), "{:?} is missing args count", target.triple);
        assert!(source.contains("beskid_rt_v5_args_get"), "{:?} is missing args get", target.triple);
        assert!(source.contains("beskid_rt_v5_args_handoff"), "{:?} is missing args handoff", target.triple);
    }
    assert!(manifest.exports.iter().any(|export| export.symbol == "beskid_rt_v5_trap"));
}
