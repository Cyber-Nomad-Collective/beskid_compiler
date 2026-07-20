use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_codegen::{lower_prepared_syntax_entrypoint, lower_prepared_syntax_module};
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};

#[test]
fn prepared_syntax_entrypoint_lowers_without_hir_host_authority() {
    let directory = std::env::temp_dir().join(format!(
        "beskid_codegen_prepared_syntax_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");
    let isa = isa::lookup_by_name("x86_64")
        .expect("x86 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("finish ISA");
    let lowered =
        with_db(|db| lower_prepared_syntax_entrypoint(db, &front, "Main", target, isa.as_ref()))
            .expect("prepared syntax lowering");

    assert_eq!(lowered.artifact.functions.len(), 2);
    assert!(lowered.symbol.starts_with("Main#syntax_"));
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn prepared_syntax_module_lowers_functions_and_methods_without_hir() {
    let directory = std::env::temp_dir().join(format!(
        "beskid_codegen_prepared_syntax_module_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Mod.bd");
    let source = "i32 Echo(i32 value) { return value; } type Worker { i32 Run(i32 value) { return value; } }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");
    let isa = isa::lookup_by_name("x86_64")
        .expect("x86 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("finish ISA");
    let artifact = with_db(|db| lower_prepared_syntax_module(db, &front, target, isa.as_ref()))
        .expect("prepared syntax module lowering");

    assert_eq!(artifact.functions.len(), 2);
    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Echo#syntax_"))
    );
    assert!(
        artifact
            .functions
            .iter()
            .any(|function| function.name.starts_with("Run#syntax_"))
    );
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn prepared_syntax_module_lowers_sample_mod_nominal_contract_methods() {
    let directory = std::env::temp_dir().join(format!(
        "beskid_codegen_prepared_syntax_sample_mod_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Mod.bd");
    let source = include_str!("../../beskid_tests/fixtures/mods/sample_mod/Src/Mod.bd");
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");
    let isa = isa::lookup_by_name("x86_64")
        .expect("x86 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("finish ISA");
    let artifact = with_db(|db| lower_prepared_syntax_module(db, &front, target, isa.as_ref()))
        .expect("prepared syntax sample Mod lowering");

    assert_eq!(artifact.functions.len(), 5);
    std::fs::remove_dir_all(directory).expect("remove project");
}
