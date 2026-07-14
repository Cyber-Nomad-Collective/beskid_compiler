use std::fs;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile as KitProfile, RuntimeKitBuildRequest, build_runtime_kit,
};
use beskid_aot::{BuildProfile, resolve_runtime_kit_archive_at_prefix};

#[test]
fn aot_resolves_only_a_validated_abi_v5_kit_archive() {
    let prefix = tempfile::tempdir().expect("prefix");
    let inputs = tempfile::tempdir().expect("inputs");
    let static_library = inputs.path().join("libbeskid_runtime.a");
    let shared_library = inputs.path().join("libbeskid_runtime.so");
    fs::write(&static_library, b"static runtime").expect("static runtime");
    fs::write(&shared_library, b"shared runtime").expect("shared runtime");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    build_runtime_kit(&RuntimeKitBuildRequest {
        prefix: prefix.path().to_path_buf(),
        target: target.clone(),
        profile: KitProfile::Debug,
        runtime_source_hash:
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        static_library: static_library.clone(),
        shared_library,
        shared_import_library: None,
    })
    .expect("build kit");

    let resolved = resolve_runtime_kit_archive_at_prefix(
        prefix.path(),
        BuildProfile::Debug,
        target.triple.as_str(),
    )
    .expect("resolve kit");
    assert_eq!(
        fs::read(resolved).expect("resolved archive"),
        fs::read(static_library).expect("source archive")
    );
}

#[test]
fn aot_rejects_a_loose_archive_without_kit_metadata() {
    let prefix = tempfile::tempdir().expect("prefix");
    let loose = prefix.path().join("libbeskid_runtime.a");
    fs::write(&loose, b"unvalidated runtime").expect("loose archive");

    let error = resolve_runtime_kit_archive_at_prefix(
        prefix.path(),
        BuildProfile::Debug,
        "x86_64-unknown-linux-gnu",
    )
    .expect_err("loose archive must not resolve");
    assert!(error.to_string().contains("ABI-v5 runtime kit"));
}
