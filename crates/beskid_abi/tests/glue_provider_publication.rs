//! Atomic packet publication tests use non-executable format fixtures, not native qualification.
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile, GLUE_PROVIDER_MANIFEST_V1, GlueProviderToolV1, RuntimeKitBuildRequest,
    build_runtime_kit_with_glue_provider, resolve_glue_shared_provider,
};
fn tools() -> Vec<GlueProviderToolV1> {
    ["native_provider_compiler", "shared_provider_linker"]
        .into_iter()
        .map(|role| GlueProviderToolV1 {
            role: role.into(),
            executable_sha256: "a".repeat(64),
            version_output_sha256: "b".repeat(64),
        })
        .collect()
}
fn request(prefix: &std::path::Path, inputs: &std::path::Path) -> RuntimeKitBuildRequest {
    let archive = inputs.join("archive");
    let shared = inputs.join("shared");
    std::fs::write(&archive, b"format-only archive").unwrap();
    std::fs::write(&shared, b"format-only shared payload").unwrap();
    RuntimeKitBuildRequest {
        prefix: prefix.to_owned(),
        target: TargetMetadata::for_triple("x86_64-unknown-linux-gnu").unwrap(),
        profile: BuildProfile::Debug,
        runtime_source_hash: beskid_abi::runtime_source::canonical_runtime_source_hash(),
        static_library: archive,
        shared_library: shared,
        shared_import_library: None,
    }
}
#[test]
fn sidecar_is_inside_atomically_published_exact_kit() {
    let prefix = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let req = request(prefix.path(), inputs.path());
    let kit = build_runtime_kit_with_glue_provider(&req, tools()).unwrap();
    assert!(kit.root.join(GLUE_PROVIDER_MANIFEST_V1).is_file());
    let resolved = resolve_glue_shared_provider(prefix.path(), &req.target, req.profile).unwrap();
    assert_eq!(resolved.shared_library, kit.shared_library);
    assert_eq!(resolved.manifest.build_tools, tools());
}
#[test]
fn invalid_tool_evidence_never_publishes_partial_kit() {
    let prefix = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let req = request(prefix.path(), inputs.path());
    assert!(build_runtime_kit_with_glue_provider(&req, Vec::new()).is_err());
    assert!(resolve_glue_shared_provider(prefix.path(), &req.target, req.profile).is_err());
    let kit = build_runtime_kit_with_glue_provider(&req, tools())
        .expect("invalid first attempt must not reserve destination");
    assert!(kit.root.join(GLUE_PROVIDER_MANIFEST_V1).is_file());
}
/// v0.6.2 8.10: a Glue shared provider published for another kit is rejected. The sidecar of a
/// kit whose shared payload differs (another build, here with another shared library) names that
/// kit's payload digest, so copying it into this kit never admits a provider.
#[test]
fn shared_provider_of_another_kit_is_rejected() {
    let first_prefix = tempfile::tempdir().unwrap();
    let first_inputs = tempfile::tempdir().unwrap();
    let first = build_runtime_kit_with_glue_provider(&request(first_prefix.path(), first_inputs.path()), tools()).unwrap();

    let second_prefix = tempfile::tempdir().unwrap();
    let second_inputs = tempfile::tempdir().unwrap();
    let mut second_request = request(second_prefix.path(), second_inputs.path());
    std::fs::write(&second_request.shared_library, b"format-only shared payload of another kit").unwrap();
    let second = beskid_abi::runtime_source::build_canonical_runtime_kit(&second_request).unwrap();
    assert!(resolve_glue_shared_provider(second_prefix.path(), &second_request.target, second_request.profile).is_err());

    std::fs::copy(first.root.join(GLUE_PROVIDER_MANIFEST_V1), second.root.join(GLUE_PROVIDER_MANIFEST_V1)).unwrap();
    let error = resolve_glue_shared_provider(second_prefix.path(), &second_request.target, second_request.profile)
        .expect_err("another kit's provider sidecar must not admit this kit's shared library");
    assert!(error.to_string().contains("identity differs"), "{error}");
    // The first kit still resolves its own provider.
    assert!(resolve_glue_shared_provider(first_prefix.path(), &second_request.target, second_request.profile).is_ok());
}
