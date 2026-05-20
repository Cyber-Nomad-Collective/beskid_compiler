use std::fs;

use beskid_aot::{
    ContractRegistration, ModArtifactBuildRequest, ModArtifactDescriptor, build_mod_artifact,
    compute_mod_artifact_key,
};
use beskid_codegen::CodegenArtifact;

#[test]
fn build_mod_artifact_writes_object_and_descriptor_under_workspace_cache() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    let project_root = workspace_root.join("mods/compiler_sdk_test_mod");
    let source_root = project_root.join("src");
    fs::create_dir_all(&source_root).expect("source root");

    let manifest_path = project_root.join("Project.proj");
    fs::write(
        &manifest_path,
        r#"
project {
  name = "compiler_sdk_test_mod"
  version = "0.0.0-local"
  root = "src"
  type = Mod
}
"#,
    )
    .expect("manifest");
    fs::write(source_root.join("mod.bd"), "contract Demo {}\n").expect("source");

    let lockfile_path = workspace_root.join("Project.lock");
    fs::write(&lockfile_path, "compiler_sdk_test_mod 0.0.0-local\n").expect("lockfile");

    let descriptor = build_mod_artifact(ModArtifactBuildRequest {
        artifact: CodegenArtifact::default(),
        workspace_root: workspace_root.clone(),
        project_root: project_root.clone(),
        manifest_path,
        source_root,
        lockfile_path: Some(lockfile_path),
        package_id: "compiler_sdk_test_mod".to_owned(),
        package_version: Some("0.0.0-local".to_owned()),
        target_triple: "aarch64-apple-darwin".to_owned(),
        compiler_version: "0.2.0-dev".to_owned(),
        registrations: Vec::<ContractRegistration>::new(),
    })
    .expect("build mod artifact");

    let expected_key = compute_mod_artifact_key(
        &descriptor.lock_hash,
        &descriptor.mod_source_hash,
        "aarch64-apple-darwin",
        "0.2.0-dev",
    );
    assert_eq!(descriptor.artifact_key, expected_key);
    assert_eq!(
        descriptor.artifact_dir,
        workspace_root
            .join(".beskid/obj/mods/compiler_sdk_test_mod")
            .join(&expected_key)
            .join("aarch64-apple-darwin")
    );
    assert_eq!(descriptor.object_file, "mod.o");
    assert!(descriptor.artifact_dir.join("mod.o").is_file());
    assert!(
        descriptor
            .artifact_dir
            .join("mod.descriptor.json")
            .is_file()
    );
    assert!(descriptor.registrations.is_empty());

    let descriptor_json = fs::read_to_string(descriptor.artifact_dir.join("mod.descriptor.json"))
        .expect("descriptor json");
    let sidecar: ModArtifactDescriptor =
        serde_json::from_str(&descriptor_json).expect("descriptor schema");
    assert_eq!(sidecar.schema_version, 1);
    assert_eq!(sidecar.package_id, "compiler_sdk_test_mod");
    assert_eq!(sidecar.package_version.as_deref(), Some("0.0.0-local"));
    assert_eq!(sidecar.mod_source_hash, descriptor.mod_source_hash);
    assert_eq!(sidecar.lock_hash, descriptor.lock_hash);
    assert_eq!(sidecar.target_triple, "aarch64-apple-darwin");
    assert_eq!(sidecar.compiler_version, "0.2.0-dev");
    assert_eq!(sidecar.object_file, "mod.o");
    assert!(sidecar.registrations.is_empty());
    assert_eq!(sidecar.artifact_key, "");
    assert_eq!(sidecar.artifact_dir, std::path::PathBuf::new());
}
