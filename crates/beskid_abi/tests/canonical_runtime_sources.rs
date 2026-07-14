use beskid_abi::abi_v5::{
    AbiManifestV5, RuntimePackageIdentity, SourceUnit, TargetMetadata, canonical_runtime_package,
};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, RuntimeCapabilityError, canonical_runtime_sources,
    grant_runtime_intrinsics,
};

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

#[test]
fn canonical_bootstrap_source_is_embedded_and_exports_the_v5_probe() {
    let sources = canonical_runtime_sources();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].logical_path, CANONICAL_BOOTSTRAP_SOURCE_PATH);
    assert!(
        sources[0]
            .source
            .contains("[Export(Abi:\"C\", Symbol:\"beskid_rt_v5_abi_version\")]")
    );
    assert!(sources[0].source.contains("return 5;"));
    assert!(!sources[0].source.contains("ABI v4"));
    assert!(!sources[0].source.contains("__"));
}

#[test]
fn exact_embedded_source_set_receives_non_serializable_intrinsic_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let capability = grant_runtime_intrinsics(&canonical_runtime_package(), &sources, &manifest)
        .expect("canonical source authority");

    assert!(capability.authorizes_source(CANONICAL_BOOTSTRAP_SOURCE_PATH));
    assert!(
        capability
            .intrinsic_for_source(CANONICAL_BOOTSTRAP_SOURCE_PATH, "trap")
            .is_some()
    );
    assert!(
        capability
            .intrinsic_for_source("src/User.bd", "trap")
            .is_none()
    );
    assert!(
        capability
            .intrinsic_for_source(CANONICAL_BOOTSTRAP_SOURCE_PATH, "not_manifest_declared",)
            .is_none()
    );
    assert_eq!(
        capability.source_hash(),
        beskid_abi::abi_v5::canonical_source_hash(&sources).unwrap()
    );
}

#[test]
fn lookalike_package_and_source_drift_cannot_receive_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let lookalike: RuntimePackageIdentity = serde_json::from_value(serde_json::json!({
        "publisher": "attacker.invalid",
        "name": "beskid-runtime-native",
        "abi_version": 5
    }))
    .unwrap();
    assert!(matches!(
        grant_runtime_intrinsics(&lookalike, &sources, &manifest),
        Err(RuntimeCapabilityError::UnauthorizedPackage)
    ));

    let mut changed = sources.clone();
    changed[0].source.push_str("\n// drift\n");
    assert!(matches!(
        grant_runtime_intrinsics(&canonical_runtime_package(), &changed, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));

    let mut extra = sources;
    extra.push(SourceUnit {
        logical_path: "src/Runtime/Backdoor.bd".into(),
        source: "pub unit Backdoor() { return; }".into(),
    });
    assert!(matches!(
        grant_runtime_intrinsics(&canonical_runtime_package(), &extra, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));
}

#[test]
fn manifest_drift_cannot_expand_runtime_authority() {
    let mut manifest = linux_manifest();
    manifest.trusted_runtime_intrinsics.pop();
    assert!(matches!(
        grant_runtime_intrinsics(
            &canonical_runtime_package(),
            &canonical_runtime_sources(),
            &manifest,
        ),
        Err(RuntimeCapabilityError::InvalidManifest)
    ));
}
