use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{ABI_V5, AbiManifestV5, RuntimeAuditMetadata, TargetMetadata};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitResolutionError,
    exact_kit_metadata_path, host_runtime_triple, installed_runtime_prefix_for_executable, installed_runtime_root,
    profile_directory_name, resolve_installed_runtime_kit,
};
use sha2::{Digest, Sha256};

struct TempPrefix(PathBuf);

static TEMP_PREFIX_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TempPrefix {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let sequence = TEMP_PREFIX_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("beskid-runtime-kit-resolution-{}-{nonce}-{sequence}", std::process::id()));
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
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn artifact(path: &str, contents: &[u8]) -> RuntimeArtifact {
    RuntimeArtifact { relative_path: path.into(), sha256: sha256(contents) }
}

fn install_linux_kit(prefix: &Path, profile: BuildProfile) -> PathBuf {
    let target = linux_target();
    let profile_name = match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    };
    let root = prefix.join("lib/beskid-runtime/abi-5").join(target.triple.as_str()).join(profile_name);
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
    fs::write(root.join("abi.json"), metadata.canonical_abi_json().unwrap()).unwrap();
    root
}

fn read_metadata(root: &Path) -> RuntimeKitMetadata {
    serde_json::from_str(&fs::read_to_string(root.join("abi.json")).unwrap()).unwrap()
}

fn write_metadata(root: &Path, metadata: &RuntimeKitMetadata) {
    fs::write(root.join("abi.json"), serde_json::to_string_pretty(metadata).unwrap()).unwrap();
}

#[test]
fn resolves_only_the_exact_installed_target_and_profile_and_verifies_artifacts() {
    let prefix = TempPrefix::new();
    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);

    let resolved = resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug).unwrap();
    assert_eq!(resolved.root, root);
    assert_eq!(resolved.static_library, root.join("static/libbeskid_runtime.a"));
    assert_eq!(resolved.shared_library, root.join("shared/libbeskid_runtime.so"));
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
    let mut metadata = read_metadata(&root);
    metadata.profile = BuildProfile::Release;
    write_metadata(&root, &metadata);
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::ProfileMismatch { .. })
    ));
}

#[test]
fn fails_closed_when_the_exact_profile_is_missing_even_if_another_profile_is_complete() {
    let prefix = TempPrefix::new();
    let debug_root = install_linux_kit(&prefix.0, BuildProfile::Debug);
    let release_root = install_linux_kit(&prefix.0, BuildProfile::Release);

    fs::remove_file(release_root.join("abi.json")).unwrap();
    assert!(debug_root.join("abi.json").is_file());
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Release),
        Err(RuntimeKitResolutionError::MetadataRead { path, .. })
            if path == release_root.join("abi.json")
    ));
}

#[test]
fn rejects_metadata_allowlist_layout_and_trap_contract_drift_before_resolving_artifacts() {
    let prefix = TempPrefix::new();
    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);

    let mut allowlist_drift = read_metadata(&root);
    allowlist_drift.import_allowlist.push("unexpected_import".into());
    write_metadata(&root, &allowlist_drift);
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataValidation(
            beskid_abi::runtime_kit::RuntimeKitValidationError::ContractAuditMismatch { field }
        )) if field == "import_allowlist"
    ));

    let mut layout_drift = read_metadata(&root);
    layout_drift.layout_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    write_metadata(&root, &layout_drift);
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataValidation(
            beskid_abi::runtime_kit::RuntimeKitValidationError::ContractLayoutHashMismatch { .. }
        ))
    ));

    let mut trap_drift = read_metadata(&root);
    trap_drift.abi_contract.traps[0].code = 99;
    write_metadata(&root, &trap_drift);
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataValidation(
            beskid_abi::runtime_kit::RuntimeKitValidationError::InvalidAbiContract
        ))
    ));
}

#[test]
fn rejects_mixed_or_hash_tampered_artifacts_instead_of_accepting_a_nearby_kit() {
    let prefix = TempPrefix::new();
    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);

    let mut mixed_artifacts = read_metadata(&root);
    mixed_artifacts.artifacts.static_library.relative_path = "shared/libbeskid_runtime.so".into();
    write_metadata(&root, &mixed_artifacts);
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataValidation(
            beskid_abi::runtime_kit::RuntimeKitValidationError::InvalidArtifactSet { .. }
        ))
    ));

    let root = install_linux_kit(&prefix.0, BuildProfile::Debug);
    fs::write(root.join("static/libbeskid_runtime.a"), b"tampered static runtime").unwrap();
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &linux_target(), BuildProfile::Debug),
        Err(RuntimeKitResolutionError::ArtifactHashMismatch { path, .. })
            if path == root.join("static/libbeskid_runtime.a")
    ));
}

#[test]
fn missing_manifest_reports_the_exact_coordinate_path_and_does_not_search() {
    let prefix = TempPrefix::new();
    let target = linux_target();
    let expected = exact_kit_metadata_path(&prefix.0, &target, BuildProfile::Debug);
    assert_eq!(
        expected,
        prefix
            .0
            .join(installed_runtime_root())
            .join(target.triple.as_str())
            .join(profile_directory_name(BuildProfile::Debug))
            .join("abi.json")
    );
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &target, BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataRead { path, .. }) if path == expected
    ));
}

#[test]
fn wrong_target_metadata_fails_closed_without_selecting_another_installed_kit() {
    let prefix = TempPrefix::new();
    let _ = install_linux_kit(&prefix.0, BuildProfile::Debug);
    let darwin = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "aarch64-apple-darwin")
        .unwrap();
    assert!(matches!(
        resolve_installed_runtime_kit(&prefix.0, &darwin, BuildProfile::Debug),
        Err(RuntimeKitResolutionError::MetadataRead { path, .. })
            if path == exact_kit_metadata_path(&prefix.0, &darwin, BuildProfile::Debug)
    ));
}

#[test]
fn install_prefix_derives_from_bin_layout_only() {
    let executable = PathBuf::from("/opt/beskid/bin/beskid_cli");
    let prefix = installed_runtime_prefix_for_executable(&executable).unwrap();
    assert_eq!(prefix, PathBuf::from("/opt/beskid"));
    assert!(
        host_runtime_triple().is_ok()
            || cfg!(not(any(
                all(target_os = "linux", target_arch = "x86_64"),
                all(target_os = "macos", target_arch = "aarch64"),
                all(target_os = "windows", target_arch = "x86_64"),
            )))
    );
}

#[test]
fn install_prefix_rejects_executables_outside_the_bin_layout() {
    let executable = PathBuf::from("/opt/beskid/tools/beskid_cli");
    assert!(installed_runtime_prefix_for_executable(&executable).is_err());
}
