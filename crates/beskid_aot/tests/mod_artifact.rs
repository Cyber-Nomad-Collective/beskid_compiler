use std::fs;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_aot::lower_prepared_syntax_entrypoint;
use beskid_aot::object_module::BeskidObjectModule;
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
fn source_only_module_cannot_issue_a_native_mod_adapter() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("Mod.bd");
    let source = "pub contract Generator { unit Generate(); } pub type Lookalike : Generator { unit Generate() {} }";
    fs::write(&source_path, source).unwrap();
    let plan = synthetic_compile_plan_for_source(&source_path);
    let resolved = resolved_input_from_plan(source_path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .unwrap();
    let target = TargetMetadata::for_triple(host_target_triple()).unwrap();
    let result = beskid_aot::lower_prepared_native_mod(&front, target);
    assert!(result.is_err(), "SDK-shaped local declarations cannot acquire canonical executable Mod ABI authority");
    assert!(!temp.path().join(".beskid/obj/mods").exists());
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
