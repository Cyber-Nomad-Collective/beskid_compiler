use std::fs;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    resolved_input_from_plan, synthetic_compile_plan_for_source, FrontEndOptions, ResolvedInput,
};
use beskid_aot::object_module::BeskidObjectModule;
use beskid_aot::{
    build_mod_artifact, compute_mod_artifact_key, lower_prepared_syntax_entrypoint, ContractRegistration,
    ModArtifactBuildRequest, ModArtifactDescriptor,
};
use beskid_codegen::CodegenArtifact;
use beskid_queries::compile_front_end_from_resolved_input;

fn host_target_triple() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
        "x86_64-pc-windows-msvc"
    } else {
        panic!("unsupported mod_artifact test host");
    }
}

#[test]
fn build_mod_artifact_writes_object_and_descriptor_under_workspace_cache() {
    let host_triple = host_target_triple();
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
        target_triple: host_triple.to_owned(),
        compiler_version: "0.2.0-dev".to_owned(),
        registrations: Vec::<ContractRegistration>::new(),
    })
    .expect("build mod artifact");

    let expected_key =
        compute_mod_artifact_key(&descriptor.lock_hash, &descriptor.mod_source_hash, host_triple, "0.2.0-dev");
    assert_eq!(descriptor.artifact_key, expected_key);
    assert_eq!(
        descriptor.artifact_dir,
        workspace_root.join(".beskid/obj/mods/compiler_sdk_test_mod").join(&expected_key).join(host_triple)
    );
    assert_eq!(descriptor.object_file, "mod.o");
    assert!(descriptor.artifact_dir.join("mod.o").is_file());
    assert!(descriptor.artifact_dir.join("mod.descriptor.json").is_file());
    assert!(descriptor.registrations.is_empty());

    let descriptor_json =
        fs::read_to_string(descriptor.artifact_dir.join("mod.descriptor.json")).expect("descriptor json");
    let sidecar: ModArtifactDescriptor = serde_json::from_str(&descriptor_json).expect("descriptor schema");
    assert_eq!(sidecar.schema_version, 1);
    assert_eq!(sidecar.package_id, "compiler_sdk_test_mod");
    assert_eq!(sidecar.package_version.as_deref(), Some("0.0.0-local"));
    assert_eq!(sidecar.mod_source_hash, descriptor.mod_source_hash);
    assert_eq!(sidecar.lock_hash, descriptor.lock_hash);
    assert_eq!(sidecar.target_triple, host_triple);
    assert_eq!(sidecar.compiler_version, "0.2.0-dev");
    assert_eq!(sidecar.object_file, "mod.o");
    assert!(sidecar.registrations.is_empty());
    assert_eq!(sidecar.artifact_key, "");
    assert_eq!(sidecar.artifact_dir, std::path::PathBuf::new());
}

#[test]
fn prepared_syntax_program_validates_and_compiles_to_object() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = r#"
i32 helper() {
    return 7;
}

i32 Main() {
    return helper();
}
"#;
    fs::write(&source_path, source).expect("write source");

    let plan = synthetic_compile_plan_for_source(&source_path);
    let resolved: ResolvedInput = resolved_input_from_plan(source_path, source.to_owned(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        None,
    )
    .expect("prepare syntax frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == host_target_triple())
        .expect("host ABI target");
    let artifact = lower_prepared_syntax_entrypoint(&front, "Main", target).expect("lower prepared syntax fixture");
    beskid_codegen::validate_artifact(&artifact).expect("validate link plan");

    let mut object = BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).expect("object module");
    object.compile_artifact(&artifact, None).expect("compile artifact");
}
