use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_codegen::{
    lower_prepared_syntax_entrypoint, lower_prepared_syntax_module, lower_syntax_assembly_entrypoint,
};
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};
use std::process::Command;

#[test]
fn prepared_syntax_entrypoint_lowers_without_hir_host_authority() {
    let directory = std::env::temp_dir().join(format!("beskid_codegen_prepared_syntax_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
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
    let lowered = with_db(|db| lower_prepared_syntax_entrypoint(db, &front, "Main", target.clone(), isa.as_ref()))
        .expect("prepared syntax lowering");
    let lowered_again =
        with_db(|db| lower_prepared_syntax_entrypoint(db, &front, "Main", target.clone(), isa.as_ref()))
            .expect("repeated prepared syntax lowering");
    let lowered_from_assembly = with_db(|db| {
        lower_syntax_assembly_entrypoint(
            db,
            std::sync::Arc::new(front.syntax_assembly()),
            "Main",
            target.clone(),
            isa.as_ref(),
        )
    })
    .expect("syntax assembly lowering");

    assert_eq!(lowered.artifact.functions.len(), 2);
    assert!(lowered.symbol.starts_with("Main#syntax_"));
    assert_eq!(lowered_again.symbol, lowered.symbol);
    assert_eq!(lowered_from_assembly.symbol, lowered.symbol);
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn compiler_trace_reports_syntax_facts_without_source_literal_payloads() {
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--ignored", "--exact", "compiler_trace_child_lowers_a_syntax_entrypoint", "--nocapture"])
        .env("BESKID_COMPILER_TRACE", "1")
        .output()
        .expect("run trace child process");
    assert!(output.status.success(), "trace child failed:\n{}", String::from_utf8_lossy(&output.stderr));

    let trace = String::from_utf8(output.stderr).expect("UTF-8 trace output");
    assert!(trace.contains("beskid-isle-trace event=entry.selected"));
    assert!(trace.contains("event=ast.node key="));
    assert!(trace.contains("event=call.fact key="));
    assert!(trace.contains("lowering=Direct"));
    assert!(
        trace.contains("callee=Item("),
        "call.fact should render DirectCallee with Item(name@path#gN:nN), got:\n{trace}"
    );
    assert!(!trace.contains("AstNodeKey {"), "call.fact must not Debug-dump AstNodeKey: {trace}");
    assert!(!trace.contains("abi_identity:"), "call.fact must not Debug-dump abi_identity numbers: {trace}");
    assert!(!trace.contains("SourceUnitId(Id("), "traces must not Debug-dump salsa SourceUnitId handles: {trace}");
    assert!(
        !trace.contains("AstNodeId(") && !trace.contains("SyntaxGenerationId("),
        "traces must use #gN:nN cursors, not Debug id wrappers: {trace}"
    );
    assert!(trace.contains("event=isle.selected"));
    assert!(trace.contains("elapsed_ms="));
    assert!(
        !trace.contains("trace-literal-payload-must-not-appear"),
        "trace must identify syntax through keys and facts, never source literal payloads: {trace}"
    );
}

#[test]
#[ignore = "run only as the isolated child process that captures compiler trace stderr"]
fn compiler_trace_child_lowers_a_syntax_entrypoint() {
    let directory = std::env::temp_dir().join(format!("beskid_codegen_trace_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    let source = "unit WriteLine(string text) { return; } unit Main() { WriteLine(\"trace-literal-payload-must-not-appear\"); return; }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
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

    with_db(|db| lower_prepared_syntax_entrypoint(db, &front, "Main", target, isa.as_ref()))
        .expect("trace fixture lowering");
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn prepared_syntax_module_lowers_functions_and_methods_without_hir() {
    let directory = std::env::temp_dir().join(format!("beskid_codegen_prepared_syntax_module_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Mod.bd");
    let source = "i32 Echo(i32 value) { return value; } type Worker { i32 Run(i32 value) { return value; } }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
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
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Echo#syntax_")));
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Run#syntax_")));
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn prepared_syntax_module_preserves_interop_export_metadata() {
    let directory = std::env::temp_dir().join(format!("beskid_codegen_prepared_syntax_exports_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Plugin.bd");
    let source = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
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

    assert!(
        artifact.exports.iter().any(|entry| {
            entry.beskid_name == "plugin_init" && entry.exported_symbol == "beskid_plugin_init" && entry.abi == "C"
        }),
        "syntax lowering must retain [Export] metadata for AOT/JIT interop"
    );
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn prepared_syntax_module_lowers_sample_mod_nominal_contract_methods() {
    let directory =
        std::env::temp_dir().join(format!("beskid_codegen_prepared_syntax_sample_mod_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Mod.bd");
    let source = include_str!("../../beskid_tests/fixtures/mods/sample_mod/Src/Mod.bd");
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
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
