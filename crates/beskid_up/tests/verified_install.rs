#![allow(non_snake_case)]
use beskid_abi::abi_v5::{ABI_V5, AbiManifestV5, RuntimeAuditMetadata, TargetMetadata};
use beskid_abi::runtime_kit::{BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata};
use beskid_up::{DirectInstall, ReleaseManifest};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const TARGET: &str = "x86_64-unknown-linux-gnu";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn kit(prefix: &Path, profile: BuildProfile) {
    let target = TargetMetadata::for_triple(TARGET).unwrap();
    let name = match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    };
    let root = prefix.join("lib/beskid-runtime/abi-5").join(TARGET).join(name);
    fs::create_dir_all(root.join("static")).unwrap();
    fs::create_dir_all(root.join("shared")).unwrap();
    fs::write(root.join("static/libbeskid_runtime.a"), b"static fixture").unwrap();
    fs::write(root.join("shared/libbeskid_runtime.so"), b"shared fixture").unwrap();
    let contract = AbiManifestV5::canonical_runtime(target.clone());
    let source = "b".repeat(64);
    let audit = RuntimeAuditMetadata::for_manifest(&contract, &source).unwrap();
    let metadata = RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target,
        profile,
        layout_hash: contract.layout_hash(),
        source_hash: source,
        artifacts: RuntimeArtifacts {
            static_library: RuntimeArtifact {
                relative_path: "static/libbeskid_runtime.a".into(),
                sha256: hash(b"static fixture"),
            },
            shared_library: RuntimeArtifact {
                relative_path: "shared/libbeskid_runtime.so".into(),
                sha256: hash(b"shared fixture"),
            },
            shared_import_library: None,
        },
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract: contract,
        audit,
    };
    fs::write(root.join("abi.json"), metadata.canonical_abi_json().unwrap()).unwrap();
}
fn fixture(version: &str, omit_release_kit: bool) -> (tempfile::TempDir, Vec<u8>, ReleaseManifest) {
    let stage = tempfile::tempdir().unwrap();
    let prefix = stage.path().join(format!("beskid-{version}-{TARGET}"));
    fs::create_dir_all(prefix.join("bin")).unwrap();
    for name in ["beskid", "beskid_lsp", "beskid-up"] {
        let path = prefix.join("bin").join(name);
        fs::write(&path, b"tool fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    fs::write(prefix.join("release-version.txt"), format!("{version}\n")).unwrap();
    let corelib = prefix.join("beskid_corelib");
    fs::create_dir_all(corelib.join("beskid_corelib")).unwrap();
    fs::create_dir_all(corelib.join("packages/foundation")).unwrap();
    fs::write(
        corelib.join("CoreLib.bws"),
        "workspace { name = fixture }\nmember \"corelib\" { path = \"beskid_corelib\" }\nmember \"foundation\" { path = \"packages/foundation\" }\n",
    )
    .unwrap();
    fs::write(corelib.join("beskid_corelib/corelib.bproj"), "fixture { name = fixture }\n").unwrap();
    fs::write(corelib.join("packages/foundation/foundation.bproj"), "foundation { name = foundation }\n").unwrap();
    let fingerprint = beskid_abi::corelib_bundle::fingerprint_corelib_bundle_dir(&corelib).unwrap();
    fs::write(corelib.join(".beskid-bundle.sha256"), fingerprint).unwrap();
    kit(&prefix, BuildProfile::Debug);
    if !omit_release_kit {
        kit(&prefix, BuildProfile::Release);
    }
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    tar.append_dir_all(prefix.file_name().unwrap(), &prefix).unwrap();
    let bytes = tar.into_inner().unwrap().finish().unwrap();
    let manifest = ReleaseManifest::from_json(&serde_json::json!({"schema":1,"version":version,"commit":"a".repeat(40),"bundles":[{"target":TARGET,"url":format!("https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v{version}/bundle.tar.gz"),"sha256":hash(&bytes)}]}).to_string()).unwrap();
    (stage, bytes, manifest)
}
#[test]
fn v06_verified_complete_install_switches_and_rejects_tampering() {
    let storage = tempfile::tempdir().unwrap();
    let store = DirectInstall::new(storage.path());
    let (_source, bytes, manifest) = fixture("0.6.0", false);
    let prefix = store.InstallArchive(&manifest, TARGET, &bytes).unwrap();
    store.activate(&manifest.version).unwrap();
    assert_eq!(store.ActivePrefix().unwrap(), Some(prefix.clone()));
    let original_active = fs::read(storage.path().join("active")).unwrap();
    let (_source2, bytes2, manifest2) = fixture("0.6.1", true);
    assert!(store.InstallArchive(&manifest2, TARGET, &bytes2).is_err());
    assert_eq!(fs::read(storage.path().join("active")).unwrap(), original_active);
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    assert!(store.InstallArchive(&manifest, TARGET, &changed).is_err());
    assert_eq!(fs::read(storage.path().join("active")).unwrap(), original_active);
    let (_source3, bytes3, manifest3) = fixture("0.6.1", false);
    let second_prefix = store.InstallArchive(&manifest3, TARGET, &bytes3).unwrap();
    store.activate(&manifest3.version).unwrap();
    assert_eq!(store.ActivePrefix().unwrap(), Some(second_prefix));
    assert!(prefix.join("bin/beskid").is_file(), "previous immutable toolchain remains installed");
    let second_active = fs::read(storage.path().join("active")).unwrap();
    fs::write(prefix.join("bin/beskid"), b"tampered").unwrap();
    assert!(store.activate(&manifest.version).is_err());
    assert_eq!(fs::read(storage.path().join("active")).unwrap(), second_active);
}
#[test]
fn v06_failed_active_replace_retains_prior_pointer() {
    let storage = tempfile::tempdir().unwrap();
    let store = DirectInstall::new(storage.path());
    let (_source, bytes, manifest) = fixture("0.6.0", false);
    store.InstallArchive(&manifest, TARGET, &bytes).unwrap();
    fs::create_dir(storage.path().join("active")).unwrap();
    fs::write(storage.path().join("active/keep"), b"prior pointer conflict").unwrap();
    assert!(store.activate(&manifest.version).is_err());
    assert_eq!(fs::read(storage.path().join("active/keep")).unwrap(), b"prior pointer conflict");
}

#[test]
fn v06_owner_receipts_bind_complete_inventory_and_active_direct_coordinate() {
    use beskid_up::{InspectInstallation, InstallationOwner, WriteOwnerReceipt};
    let storage = tempfile::tempdir().unwrap();
    let store = DirectInstall::new(storage.path());
    let (_source, bytes, manifest) = fixture("0.6.0", false);
    let prefix = store.InstallArchive(&manifest, TARGET, &bytes).unwrap();
    let executable = prefix.join("bin/beskid");
    let inactive = InspectInstallation(&executable, &store).unwrap();
    assert_eq!(inactive.owner, Some(InstallationOwner::Direct));
    assert!(!inactive.direct_update_allowed);
    store.activate(&manifest.version).unwrap();
    assert!(InspectInstallation(&executable, &store).unwrap().direct_update_allowed);
    assert!(WriteOwnerReceipt(&prefix, InstallationOwner::Debian, &manifest.version, TARGET).is_err());
    fs::write(prefix.join("unlisted-payload"), b"foreign").unwrap();
    assert!(InspectInstallation(&executable, &store).is_err());
}

#[test]
fn v06_package_owner_receipt_rejects_tampering_unknown_fields_and_conflicting_direct_receipt() {
    use beskid_up::{InspectInstallation, InstallationOwner, WriteOwnerReceipt};
    let (source, _bytes, manifest) = fixture("0.6.0", false);
    let prefix = source.path().join(format!("beskid-0.6.0-{TARGET}"));
    let unrelated = tempfile::tempdir().unwrap();
    let store = DirectInstall::new(unrelated.path());
    WriteOwnerReceipt(&prefix, InstallationOwner::Debian, &manifest.version, TARGET).unwrap();
    let executable = prefix.join("bin/beskid");
    let status = InspectInstallation(&executable, &store).unwrap();
    assert_eq!(status.owner, Some(InstallationOwner::Debian));
    assert!(!status.direct_update_allowed);
    assert!(status.update_guidance.contains("apt install ./beskid-"));
    let receipt_path = prefix.join(".beskid-owner.json");
    let original = fs::read(&receipt_path).unwrap();
    let mut receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    receipt["untrusted-command"] = serde_json::json!("run me");
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(InspectInstallation(&executable, &store).is_err());
    fs::write(&receipt_path, &original).unwrap();
    fs::write(prefix.join(".beskid-install.json"), b"{}").unwrap();
    assert!(InspectInstallation(&executable, &store).is_err());
    fs::remove_file(prefix.join(".beskid-install.json")).unwrap();
    fs::write(executable, b"tampered").unwrap();
    assert!(InspectInstallation(&prefix.join("bin/beskid"), &store).is_err());
}
