//! End-to-end `beskid mod rebuild` descriptor extraction and host dispatch.

use std::fs;

use beskid_analysis::mod_host::{
    ModHostInput, NativeContractInvoker, StubContractInvoker,
    extract_mod_contract_registrations_from_syntax, run_through_generate,
};
use beskid_analysis::projects::{
    ProjectKind, WorkspacePrepareOptions, build_compile_plan, load_manifest_from_path,
    prepare_project_workspace_with_options,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_aot::{ModArtifactBuildRequest, build_mod_artifact};
use beskid_engine::services::prepare_jit_module;

use super::fixture::ModFixtureWorkspace;

#[test]
fn sample_mod_rebuild_writes_descriptor_registrations_and_host_dispatches() {
    let workspace = ModFixtureWorkspace::new("sample_mod_rebuild_e2e");
    let manifest_path = workspace.mod_dir.join("SampleMod.bproj");
    let manifest = load_manifest_from_path(&manifest_path).expect("load mod manifest");
    assert_eq!(manifest.project.kind, ProjectKind::Mod);

    let plan = build_compile_plan(&manifest_path, None).expect("compile plan");
    let prepared = prepare_project_workspace_with_options(
        &plan,
        WorkspacePrepareOptions {
            frozen: false,
            locked: false,
        },
        None,
    )
    .expect("prepare mod workspace");

    let source_path = workspace.mod_dir.join("Src").join("Mod.bd");
    let source = fs::read_to_string(&source_path).expect("read mod source");
    let program = parse_program_with_source_name("Mod.bd", &source).expect("parse mod source");
    let registrations = extract_mod_contract_registrations_from_syntax("SampleMod", &program);
    assert!(
        registrations.len() >= 5,
        "expected non-empty descriptor registrations, got: {registrations:?}"
    );

    let target =
        beskid_aot::target::detect_target(Some(host_target_triple())).expect("host target");
    let descriptor = build_mod_artifact(ModArtifactBuildRequest {
        artifact: prepare_jit_module(&source_path, &source)
            .expect("lower mod project through syntax codegen"),
        workspace_root: workspace.host_dir.clone(),
        project_root: workspace.mod_dir.clone(),
        manifest_path,
        source_root: workspace.mod_dir.join("Src"),
        lockfile_path: Some(prepared.lockfile_path),
        package_id: "SampleMod".to_owned(),
        package_version: Some("0.1.0".to_owned()),
        target_triple: target.triple,
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        registrations: registrations
            .into_iter()
            .map(
                |registration| beskid_aot::mod_artifact::ContractRegistration {
                    contract_id: registration.contract_id,
                    type_id: registration.type_id,
                    entry_symbol: registration.entry_symbol,
                },
            )
            .collect(),
    })
    .expect("build mod artifact");

    let descriptor_json = fs::read_to_string(descriptor.sidecar_path()).expect("read descriptor");
    assert!(
        descriptor_json.contains("\"registrations\""),
        "descriptor sidecar must include registrations"
    );
    assert!(
        !descriptor_json.contains("\"registrations\": []"),
        "descriptor registrations must be non-empty: {descriptor_json}"
    );

    let host_source = workspace.host_source();
    let host_plan = workspace.compile_plan();
    let stub_invoker = StubContractInvoker::new();
    let program = parse_program_with_source_name("Main.bd", host_source).expect("parse host");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&host_plan),
            source_name: "Main.bd",
            source: host_source,
            pipeline: None,
            invoker: Some(&stub_invoker),
            cached_target_fingerprint: None,
        },
    )
    .expect("host mod dispatch");

    assert_eq!(generated.session.loaded_descriptor_count(), 1);
    assert!(!generated.collector_outcomes.is_empty());
    assert!(!generated.generator_outcomes.is_empty());
    assert!(
        !stub_invoker.invocations().is_empty(),
        "host must dispatch contracts from rebuilt descriptor"
    );

    let native_invoker = NativeContractInvoker::new(vec![descriptor.object_path()]);
    let program_native =
        parse_program_with_source_name("Main.bd", host_source).expect("parse host again");
    let native_generated = run_through_generate(
        program_native,
        &ModHostInput {
            compile_plan: Some(&host_plan),
            source_name: "Main.bd",
            source: host_source,
            pipeline: None,
            invoker: Some(&native_invoker),
            cached_target_fingerprint: None,
        },
    )
    .expect("native invoker dispatch");
    assert_eq!(
        native_generated.session.loaded_descriptor_count(),
        generated.session.loaded_descriptor_count()
    );
    assert!(
        !native_invoker.invocations().is_empty(),
        "native invoker must schedule contracts from descriptor"
    );
}

fn host_target_triple() -> &'static str {
    if cfg!(target_os = "macos") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu"
    } else {
        "x86_64-pc-windows-msvc"
    }
}
