use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{ABI_V5, AbiManifestV5, RuntimeAuditMetadata, TargetMetadata};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitResolutionError,
    resolve_installed_runtime_kit,
};
use sha2::{Digest, Sha256};

struct TempPrefix(PathBuf);

static TEMP_PREFIX_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TempPrefix {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_PREFIX_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beskid-runtime-kit-resolution-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempPrefix {
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

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn artifact(path: &str, contents: &[u8]) -> RuntimeArtifact {
    RuntimeArtifact {
        relative_path: path.into(),
        sha256: sha256(contents),
    }
}

fn install_linux_kit(prefix: &Path, profile: BuildProfile) -> PathBuf {
    let target = linux_target();
    let profile_name = match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    };
    let root = prefix
        .join("lib/beskid-runtime/abi-5")
        .join(target.triple.as_str())
        .join(profile_name);
    let static_bytes = b"static runtime";
    let shared_bytes = b"shared runtime";
    fs::create_dir_all(root.join("static")).unwrap();
    fs::create_dir_all(root.join("shared")).unwrap();
    fs::write(root.join("static/libbeskid_runtime.a"), static_bytes).unwrap();
    fs::write(root.join("shared/libbeskid_runtime.so"), shared_bytes).unwrap();

    let abi_contract = AbiManifestV5::canonical_runtime(target.clone());
    let source_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let audit = RuntimeAuditMetadata::for_manifest(&abi_contract, source_hash).unwrap();
    let metadata = RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target,
        profile,
        layout_hash: abi_contract.layout_hash(),
        source_hash: source_hash.into(),
        artifacts: RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a", static_bytes),
            shared_library: artifact("shared/libbeskid_runtime.so", shared_bytes),
            shared_import_library: None,
        },
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract,
        audit,
    };
    fs::write(
        root.join("abi.json"),
        metadata.canonical_abi_json().unwrap(),
    )
    .unwrap();
    root
}

#[test]
fn resolves_only_the_exact_installed_target_and_profile_and_verifies_artifacts() {
    let prefix = TempPrefix::new();
    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);

    let resolved =
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug).unwrap();
    assert_eq!(resolved.root, root);
    assert_eq!(
        resolved.static_library,
        root.join("static/libbeskid_runtime.a")
    );
    assert_eq!(
        resolved.shared_library,
        root.join("shared/libbeskid_runtime.so")
    );
    assert_eq!(resolved.shared_import_library, None);

    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Release),
        Err(RuntimeKitResolutionError::MetadataRead { .. })
    ));
}

#[test]
fn rejects_metadata_identity_and_artifact_hash_mismatches_without_fallback() {
    let prefix = TempPrefix::new();
    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);

    fs::write(root.join("shared/libbeskid_runtime.so"), b"tampered").unwrap();
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::ArtifactHashMismatch { .. })
    ));

    install_linux_kit(&prefix.0, BuildProfile::Debug);
    let mut metadata: RuntimeKitMetadata =
        serde_json::from_str(&fs::read_to_string(root.join("abi.json")).unwrap()).unwrap();
    metadata.profile = BuildProfile::Release;
    fs::write(
        root.join("abi.json"),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::ProfileMismatch { .. })
    ));
}
