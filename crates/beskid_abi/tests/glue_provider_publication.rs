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
