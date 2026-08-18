use beskid_abi::abi_v5::{AbiManifestV5, AbiType, SourceUnit, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
    CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_SOURCE_PATH,
    CANONICAL_CLOCKS_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_PROCESS_SOURCE_PATH, CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH, CANONICAL_SCHEDULER_CORE_SOURCE_PATH,
    CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH, CANONICAL_SCHEDULER_LOOP_SOURCE_PATH,
    CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH, CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH,
    CANONICAL_SYSCALLS_SOURCE_PATH, RuntimeCapabilityError, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_runtime_intrinsic_capability, canonical_runtime_sources,
    prove_canonical_runtime_corpus,
};

fn linux_manifest() -> AbiManifestV5 {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux target");
    AbiManifestV5::canonical_runtime(target)
}

#[test]
fn canonical_bootstrap_sources_exist() {
    let sources = canonical_runtime_sources();
    for path in [
        CANONICAL_BOOTSTRAP_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH,
    ] {
        assert!(sources.iter().any(|unit| unit.logical_path == path), "canonical runtime source {path} must exist",);
    }
}

#[test]
fn canonical_scheduler_sources_exist() {
    let sources = canonical_runtime_sources();
    for path in [
        CANONICAL_SCHEDULER_SOURCE_PATH,
        CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
        CANONICAL_SCHEDULER_CORE_SOURCE_PATH,
        CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH,
        CANONICAL_SCHEDULER_QUEUE_SOURCE_PATH,
        CANONICAL_SCHEDULER_LOOP_SOURCE_PATH,
        CANONICAL_SCHEDULER_EXPORTS_SOURCE_PATH,
    ] {
        assert!(sources.iter().any(|unit| unit.logical_path == path), "canonical runtime source {path} must exist",);
    }
}

#[test]
fn canonical_host_sources_exist() {
    let sources = canonical_runtime_sources();
    for path in [CANONICAL_CLOCKS_SOURCE_PATH, CANONICAL_PROCESS_SOURCE_PATH, CANONICAL_SYSCALLS_SOURCE_PATH] {
        assert!(sources.iter().any(|unit| unit.logical_path == path), "canonical runtime source {path} must exist",);
    }
}

#[test]
fn exact_embedded_source_set_receives_non_serializable_intrinsic_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();
    let proof = prove_canonical_runtime_corpus(&sources, &manifest).expect("canonical source proof");
    let capability = canonical_runtime_intrinsic_capability(&manifest).expect("canonical intrinsic authority");

    for logical_path in [
        CANONICAL_BOOTSTRAP_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH,
    ] {
        assert!(proof.authorizes_source(logical_path));
        assert!(capability.authorizes_source(logical_path));
    }
    for intrinsic in [
        "system_allocate",
        "system_free",
        "tls_get",
        "tls_set",
        "pointer_add",
        "raw_word_load",
        "raw_word_store",
        "memory_set",
        "trap",
    ] {
        assert!(
            capability.intrinsic_for_source(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, intrinsic).is_some(),
            "canonical runtime must retain authority for {intrinsic}",
        );
    }
    assert!(capability.intrinsic_for_source("src/User.bd", "trap").is_none());
    assert!(
        capability.intrinsic_for_source(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, "not_manifest_declared",).is_none()
    );
    assert_eq!(capability.source_hash(), beskid_abi::abi_v5::canonical_source_hash(&sources).unwrap());
}

#[test]
fn lookalike_source_path_name_or_contents_cannot_receive_authority() {
    let manifest = linux_manifest();
    let sources = canonical_runtime_sources();

    let mut changed = sources.clone();
    changed[0].source.push_str("\n// drift\n");
    assert!(matches!(
        prove_canonical_runtime_corpus(&changed, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));

    let mut renamed = sources.clone();
    renamed[0].logical_path = "src/User/Bootstrap.bd".into();
    assert!(matches!(
        prove_canonical_runtime_corpus(&renamed, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));

    let mut extra = sources;
    extra.push(SourceUnit {
        logical_path: "src/Runtime/Backdoor.bd".into(),
        source: "pub unit Backdoor() { return; }".into(),
    });
    assert!(matches!(
        prove_canonical_runtime_corpus(&extra, &manifest),
        Err(RuntimeCapabilityError::SourceSetMismatch)
    ));
}

#[test]
fn manifest_drift_cannot_expand_runtime_authority() {
    let mut manifest = linux_manifest();
    manifest.trusted_runtime_intrinsics.pop();
    assert!(matches!(canonical_runtime_intrinsic_capability(&manifest), Err(RuntimeCapabilityError::InvalidManifest)));
}

#[test]
fn canonical_foundation_assert_owns_only_the_panic_service() {
    let capability = canonical_corelib_service_capability(&linux_manifest()).expect("Corelib service authority");

    assert_eq!(
        capability
            .service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__panic_str")
            .map(|service| service.symbol),
        Some("panic_str")
    );
    assert!(
        capability.service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__syscall_write").is_none(),
        "the Assert unit must not receive every Corelib service"
    );
}

#[test]
fn canonical_corelib_service_source_paths_are_lexically_normalized() {
    for logical in [CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH] {
        let path =
            canonical_corelib_service_source_path(logical).unwrap_or_else(|| panic!("missing path for {logical}"));
        assert!(
            !path.components().any(|component| matches!(component, std::path::Component::ParentDir)),
            "service path for {logical} retained ParentDir: {path:?}"
        );
        assert!(
            path.ends_with(std::path::Path::new(logical)),
            "normalized path for {logical} lost its relative suffix: {path:?}"
        );
    }
}
