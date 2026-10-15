//! Glue must never mint shared-provider authority from a missing/static-only kit.
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{BuildProfile, canonical_glue_issuer_source_sha256, resolve_glue_shared_provider};

#[test]
fn issuer_source_authority_is_embedded_and_deterministic() {
    let expected = canonical_glue_issuer_source_sha256();
    assert_eq!(expected.len(), 64);
    assert!(expected.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(expected, canonical_glue_issuer_source_sha256());
}

#[test]
fn missing_provider_never_falls_back_to_static_runtime() {
    let temporary = tempfile::tempdir().expect("owned test prefix");
    for target in TargetMetadata::supported() {
        assert!(resolve_glue_shared_provider(temporary.path(), &target, BuildProfile::Debug).is_err());
    }
}

fn packet_fixture(
    target: TargetMetadata,
) -> (tempfile::TempDir, beskid_abi::runtime_kit::ResolvedRuntimeKit, beskid_abi::runtime_kit::GlueProviderManifestV1) {
    use beskid_abi::runtime_kit::{GlueProviderManifestV1, GlueProviderToolV1, RuntimeKitBuildRequest};
    let prefix = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    // These are format-reader fixtures, never executable/native qualification artifacts.
    let archive = inputs.path().join("archive");
    let shared = inputs.path().join("shared");
    std::fs::write(&archive, b"reader-only static payload").unwrap();
    std::fs::write(&shared, b"reader-only shared payload").unwrap();
    let import = if target.object_format.as_str() == "coff" {
        let path = inputs.path().join("import");
        std::fs::write(&path, b"reader-only COFF import payload").unwrap();
        Some(path)
    } else {
        None
    };
    let request = RuntimeKitBuildRequest {
        prefix: prefix.path().to_owned(),
        target,
        profile: BuildProfile::Debug,
        runtime_source_hash: beskid_abi::runtime_source::canonical_runtime_source_hash(),
        static_library: archive,
        shared_library: shared,
        shared_import_library: import,
    };
    let kit = beskid_abi::runtime_source::build_canonical_runtime_kit(&request).unwrap();
    let manifest = GlueProviderManifestV1 {
        schema_version: 1,
        issuer_version: 1,
        target: kit.metadata.target.triple.as_str().into(),
        profile: kit.metadata.profile,
        runtime_layout_sha256: kit.metadata.layout_hash.clone(),
        runtime_source_sha256: kit.metadata.source_hash.clone(),
        issuer_source_sha256: canonical_glue_issuer_source_sha256(),
        shared_library: kit.metadata.artifacts.shared_library.clone(),
        shared_import_library: kit.metadata.artifacts.shared_import_library.clone(),
        build_tools: ["native_provider_compiler", "shared_provider_linker"]
            .into_iter()
            .map(|role| GlueProviderToolV1 {
                role: role.into(),
                executable_sha256: "a".repeat(64),
                version_output_sha256: "b".repeat(64),
            })
            .collect(),
    };
    (prefix, kit, manifest)
}
fn write_packet(
    kit: &beskid_abi::runtime_kit::ResolvedRuntimeKit,
    packet: &beskid_abi::runtime_kit::GlueProviderManifestV1,
) {
    std::fs::write(
        kit.root.join(beskid_abi::runtime_kit::GLUE_PROVIDER_MANIFEST_V1),
        serde_json::to_vec(packet).unwrap(),
    )
    .unwrap();
}

#[test]
fn canonical_issuer_hash_matches_independently_framed_source_files() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/Glue");
    let mut framed = b"beskid.Glue.OwnerIssuer.V1\0".to_vec();
    for name in ["owner_identity_v1.c", "owner_identity_v1.h"] {
        let bytes = std::fs::read(root.join(name)).unwrap();
        framed.extend_from_slice(&(name.len() as u64).to_le_bytes());
        framed.extend_from_slice(name.as_bytes());
        framed.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        framed.extend_from_slice(&bytes);
    }
    assert_eq!(canonical_glue_issuer_source_sha256(), format!("{:x}", Sha256::digest(&framed)));
    framed.push(0);
    assert_ne!(canonical_glue_issuer_source_sha256(), format!("{:x}", Sha256::digest(&framed)));
}

#[test]
fn reader_admits_exact_packet_and_returns_shared_or_coff_import_link_path() {
    for target in TargetMetadata::supported() {
        let (prefix, kit, packet) = packet_fixture(target.clone());
        write_packet(&kit, &packet);
        let provider = resolve_glue_shared_provider(prefix.path(), &target, BuildProfile::Debug).unwrap();
        assert_eq!(provider.shared_library, kit.shared_library);
        assert_eq!(provider.link_library, kit.shared_import_library.unwrap_or(kit.shared_library));
        assert_ne!(provider.link_library, kit.static_library);
    }
}

#[test]
fn reader_rejects_each_identity_and_tool_closure_substitution() {
    let target = TargetMetadata::for_triple("x86_64-pc-windows-msvc").unwrap();
    let (prefix, kit, packet) = packet_fixture(target.clone());
    let mut variants = Vec::new();
    macro_rules! changed {
        ($field:ident, $value:expr) => {{
            let mut p = packet.clone();
            p.$field = $value;
            variants.push(p);
        }};
    }
    changed!(schema_version, 2);
    changed!(issuer_version, 2);
    changed!(target, "aarch64-apple-darwin".into());
    changed!(profile, BuildProfile::Release);
    changed!(runtime_layout_sha256, "c".repeat(64));
    changed!(runtime_source_sha256, "c".repeat(64));
    changed!(issuer_source_sha256, "c".repeat(64));
    changed!(shared_import_library, None);
    changed!(build_tools, Vec::new());
    let mut p = packet.clone();
    p.shared_library.sha256 = "c".repeat(64);
    variants.push(p);
    let mut p = packet.clone();
    p.shared_library.relative_path = kit.metadata.artifacts.static_library.relative_path.clone();
    variants.push(p);
    let mut p = packet.clone();
    p.shared_import_library.as_mut().unwrap().sha256 = "c".repeat(64);
    variants.push(p);
    let mut p = packet.clone();
    p.build_tools.reverse();
    variants.push(p);
    let mut p = packet.clone();
    p.build_tools[1].role = p.build_tools[0].role.clone();
    variants.push(p);
    let mut p = packet.clone();
    p.build_tools[0].executable_sha256 = "not-a-digest".into();
    variants.push(p);
    let mut p = packet.clone();
    p.build_tools[1].version_output_sha256 = "C".repeat(64);
    variants.push(p);
    for invalid in variants {
        write_packet(&kit, &invalid);
        assert!(
            resolve_glue_shared_provider(prefix.path(), &target, BuildProfile::Debug).is_err(),
            "mismatch admitted: {invalid:?}"
        );
    }
}

#[test]
fn reader_rejects_unknown_fields_oversize_and_changed_actual_payload() {
    let target = TargetMetadata::for_triple("x86_64-unknown-linux-gnu").unwrap();
    let (prefix, kit, packet) = packet_fixture(target.clone());
    let path = kit.root.join(beskid_abi::runtime_kit::GLUE_PROVIDER_MANIFEST_V1);
    let mut json = serde_json::to_value(&packet).unwrap();
    json["alternate_registry"] = true.into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(resolve_glue_shared_provider(prefix.path(), &target, BuildProfile::Debug).is_err());
    std::fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(resolve_glue_shared_provider(prefix.path(), &target, BuildProfile::Debug).is_err());
    write_packet(&kit, &packet);
    std::fs::write(&kit.shared_library, b"substituted actual image").unwrap();
    assert!(resolve_glue_shared_provider(prefix.path(), &target, BuildProfile::Debug).is_err());
}
