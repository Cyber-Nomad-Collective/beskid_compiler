use std::fs;

use beskid_abi::abi_v5::{TargetMetadata, canonical_source_hash};
use beskid_abi::runtime_kit::{
    BuildProfile as KitProfile, RuntimeKitBuildRequest, build_runtime_kit,
};
use beskid_abi::runtime_source::canonical_runtime_sources;
use beskid_aot::api::BuildProfile;
use beskid_aot::bundled::{installed_runtime_strategy, resolve_installed_runtime_archive};
use beskid_aot::runtime::{RuntimeBuildRequest, prepare_runtime};

fn linux_target() -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux ABI-v5 target")
}

fn install_kit_with_source_hash(prefix: &std::path::Path, runtime_source_hash: String) {
    let inputs = prefix.join("inputs");
    fs::create_dir_all(&inputs).unwrap();
    let static_library = inputs.join("runtime.a");
    let shared_library = inputs.join("runtime.so");
    fs::write(&static_library, b"hermetic static runtime").unwrap();
    fs::write(&shared_library, b"hermetic shared runtime").unwrap();
    build_runtime_kit(&RuntimeKitBuildRequest {
        prefix: prefix.to_path_buf(),
        target: linux_target(),
        profile: KitProfile::Debug,
        runtime_source_hash,
        static_library,
        shared_library,
        shared_import_library: None,
    })
    .expect("install exact test kit");
}

fn install_kit(prefix: &std::path::Path) {
    install_kit_with_source_hash(
        prefix,
        canonical_source_hash(&canonical_runtime_sources()).unwrap(),
    );
}

#[test]
fn aot_preparation_resolves_only_the_validated_static_artifact_and_allowlist() {
    let temp = tempfile::tempdir().unwrap();
    install_kit(temp.path());
    let strategy = installed_runtime_strategy(
        temp.path(),
        BuildProfile::Debug,
        Some("x86_64-unknown-linux-gnu"),
    )
    .expect("exact kit strategy");
    let prepared = prepare_runtime(&RuntimeBuildRequest { kit: strategy }).expect("validated kit");
    let resolved = resolve_installed_runtime_archive(
        temp.path(),
        BuildProfile::Debug,
        Some("x86_64-unknown-linux-gnu"),
    )
    .expect("validated static library");

    assert_eq!(prepared.staticlib_path, resolved);
    assert!(
        prepared
            .exported_symbols
            .contains(&"beskid_rt_v5_abi_version".to_owned())
    );
    assert!(
        prepared
            .exported_symbols
            .contains(&"beskid_arch_v5_context_switch".to_owned())
    );
}

#[test]
fn tampered_or_wrong_profile_kits_fail_without_archive_fallback() {
    let temp = tempfile::tempdir().unwrap();
    install_kit(temp.path());
    let debug = installed_runtime_strategy(
        temp.path(),
        BuildProfile::Debug,
        Some("x86_64-unknown-linux-gnu"),
    )
    .unwrap();
    assert_eq!(debug.prefix, temp.path());

    let static_path = temp
        .path()
        .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug/static/libbeskid_runtime.a");
    fs::write(&static_path, b"tampered").unwrap();
    assert!(prepare_runtime(&RuntimeBuildRequest { kit: debug }).is_err());

    let release = installed_runtime_strategy(
        temp.path(),
        BuildProfile::Release,
        Some("x86_64-unknown-linux-gnu"),
    )
    .unwrap();
    assert!(prepare_runtime(&RuntimeBuildRequest { kit: release }).is_err());
}

#[test]
fn tampered_exact_kit_does_not_fall_back_to_a_legacy_prebuilt_archive() {
    let temp = tempfile::tempdir().unwrap();
    install_kit(temp.path());
    let strategy = installed_runtime_strategy(
        temp.path(),
        BuildProfile::Debug,
        Some("x86_64-unknown-linux-gnu"),
    )
    .expect("exact kit strategy");

    let exact_static = temp
        .path()
        .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug/static/libbeskid_runtime.a");
    fs::write(&exact_static, b"tampered exact runtime").unwrap();

    // This is the former workspace/prebuilt fallback shape. Its presence must never authorize
    // linking after the exact kit has failed validation.
    let legacy_prebuilt = temp.path().join("target/debug/libbeskid_runtime_bridge.a");
    fs::create_dir_all(legacy_prebuilt.parent().unwrap()).unwrap();
    fs::write(&legacy_prebuilt, b"legacy prebuilt runtime").unwrap();

    assert!(
        prepare_runtime(&RuntimeBuildRequest { kit: strategy }).is_err(),
        "AOT must reject a failed exact kit instead of falling back to a prebuilt archive"
    );
}

#[test]
fn noncanonical_targets_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        installed_runtime_strategy(
            temp.path(),
            BuildProfile::Debug,
            Some("x86_64-unknown-linux-musl"),
        )
        .is_err()
    );
}

#[test]
fn internally_valid_kit_for_another_runtime_source_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    install_kit_with_source_hash(temp.path(), "a".repeat(64));
    let strategy = installed_runtime_strategy(
        temp.path(),
        BuildProfile::Debug,
        Some("x86_64-unknown-linux-gnu"),
    )
    .unwrap();
    assert!(prepare_runtime(&RuntimeBuildRequest { kit: strategy }).is_err());
}
