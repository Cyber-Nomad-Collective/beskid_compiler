use std::sync::atomic::{AtomicU64, Ordering};

use super::{PrepareOptions, collect_analyzer_diagnostics, prepare_compilation};
use crate::analysis::diagnostics::Severity;
use crate::mod_host::{
    AnalyzerDiagnostic, AnalyzerSeverity, ContractInvoker, ContractRegistration, ModHostAnalyzeResult,
    ModInvocationContext, ScriptedContractInvoker,
};
use crate::projects::SourceUnit;
use crate::services::semantic::require_no_semantic_errors;
use crate::services::{
    FrontEndOptions, parse_program_with_source_name, resolved_input_from_plan, synthetic_compile_plan_for_source,
};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn prepare_spine_syntax_assembly_uses_rewritten_entry_without_document_snapshot() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_syntax_assembly_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "i32 Main() { return 0; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let prepared = prepare_compilation(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..PrepareOptions::without_mods()
        },
        None,
    )
    .expect("prepare");

    // Typed prepare projects through FrontEndTypedResult::syntax_assembly (post-rewrite entry).
    let syntax = prepared.syntax_assembly();
    assert_eq!(
        syntax.entry_unit().program,
        prepared.program,
        "prepare-spine syntax assembly must match the prepare entry program"
    );
    let typed = prepared.typed.as_ref().expect("typed front-end");
    assert_eq!(syntax.entry_unit().program, typed.syntax_assembly().entry_unit().program);

    // Untyped path: rewritten prepare.program is the sole authority (no DocumentAnalysisSnapshot).
    let rewritten = parse_program_with_source_name("Main.bd", "i32 Rewritten() { return 1; }").expect("rewritten");
    let mut untyped = prepared;
    untyped.typed = None;
    untyped.program = rewritten.clone();
    assert_eq!(
        untyped.syntax_assembly().entry_unit().program,
        rewritten,
        "untyped prepare-spine syntax assembly must project prepare.program"
    );
    assert!(!untyped.syntax_assembly().units.is_empty(), "syntax assembly must retain immutable source units");

    let _ = std::fs::remove_dir_all(root);
}

/// Build a `ModHostAnalyzeResult` carrying one scripted `Analyzer` outcome so the
/// prepare-spine mod-diagnostic gate can be exercised without a native mod artifact.
fn mod_rewrite_with_scripted_analyzer(
    type_id: &str,
    diagnostic: AnalyzerDiagnostic,
    entry_source: &str,
) -> (ModHostAnalyzeResult, SourceUnit) {
    let invoker = ScriptedContractInvoker::new().with_analyzer_diagnostic(type_id, vec![diagnostic]);
    let registration = ContractRegistration {
        contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
        type_id: type_id.to_owned(),
        entry_symbol: "moda_check".to_owned(),
    };
    let context = ModInvocationContext::empty();
    let outcome = invoker
        .invoke_analyzer(&registration, &context.collect_request, None, None)
        .expect("scripted analyzer invocation");
    assert_eq!(outcome.diagnostics.len(), 1, "scripted analyzer diagnostic must overlay");

    let program = parse_program_with_source_name("Main.bd", entry_source).expect("parse entry");
    let entry_unit = SourceUnit {
        logical_name: "Main.bd".to_owned(),
        origin_path: std::path::PathBuf::from("/tmp/Main.bd"),
        path: std::path::PathBuf::from("/tmp/Main.bd"),
        source: entry_source.to_owned(),
        program,
    };
    let mod_rewrite = ModHostAnalyzeResult {
        program: entry_unit.program.clone(),
        analyzer_outcomes: vec![outcome],
        rewriter_outcomes: Vec::new(),
        edited_source: None,
    };
    (mod_rewrite, entry_unit)
}

/// A mod `Error`-severity diagnostic must fail the typed/codegen build path
/// (`require_no_semantic_errors` is the same gate used for compiler semantic errors).
#[test]
fn mod_error_diagnostic_fails_typed_build_path() {
    let entry_source = "unit Main() { return; }\n";
    let diagnostic = AnalyzerDiagnostic {
        code: "MOD0001".to_owned(),
        message: "mod error".to_owned(),
        severity: AnalyzerSeverity::Error,
        span: Some((0, 4)),
    };
    let (mod_rewrite, entry_unit) = mod_rewrite_with_scripted_analyzer("ModA.Check", diagnostic, entry_source);

    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
    assert_eq!(analyzer_diagnostics.len(), 1);
    assert_eq!(analyzer_diagnostics[0].severity, Severity::Error);
    assert_eq!(analyzer_diagnostics[0].origin.as_deref(), Some("beskid:mod:ModA.Check"));

    // Typed/codegen path gate: mod Error fails the build.
    assert!(
        require_no_semantic_errors(&analyzer_diagnostics).is_err(),
        "mod Error-severity diagnostic must fail the typed build path"
    );
}

/// A mod `Warning`-severity diagnostic must NOT fail the typed/codegen build path
/// (Warnings/Notes are dropped on the build path, matching the compiler-diagnostic contract).
#[test]
fn mod_warning_diagnostic_does_not_fail_typed_build_path() {
    let entry_source = "unit Main() { return; }\n";
    let diagnostic = AnalyzerDiagnostic {
        code: "MOD0002".to_owned(),
        message: "mod warning".to_owned(),
        severity: AnalyzerSeverity::Warning,
        span: Some((0, 4)),
    };
    let (mod_rewrite, entry_unit) = mod_rewrite_with_scripted_analyzer("ModA.Check", diagnostic, entry_source);

    let analyzer_diagnostics = collect_analyzer_diagnostics(&mod_rewrite.analyzer_outcomes, &entry_unit, entry_source);
    assert_eq!(analyzer_diagnostics.len(), 1);
    assert_eq!(analyzer_diagnostics[0].severity, Severity::Warning);
    assert_eq!(analyzer_diagnostics[0].origin.as_deref(), Some("beskid:mod:ModA.Check"));

    // Typed/codegen path gate: mod Warning does not fail the build.
    assert!(
        require_no_semantic_errors(&analyzer_diagnostics).is_ok(),
        "mod Warning-severity diagnostic must not fail the typed build path"
    );
}
#[test]
fn cached_editor_assembly_cannot_enter_strict_prepare_and_projections_preserve_policy() {
    use crate::projects::AssemblyRecoveryPolicy;
    let directory = tempfile::tempdir().expect("isolated policy project");
    let entry_path = directory.path().join("Main.bd");
    let source = "i32 Main() { return 0; }";
    std::fs::write(&entry_path, source).unwrap();
    let plan = synthetic_compile_plan_for_source(&entry_path);
    let mut assembly_options =
        crate::projects::assembly_options_for_prepare(&plan, FrontEndOptions::default().assembly_discovery);
    assembly_options.recovery_policy = AssemblyRecoveryPolicy::EditorRetainRecovered;
    let cached = crate::projects::assemble_program(&plan, None, &entry_path, Some(source), &assembly_options, None)
        .expect("actual editor-policy assembly");
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_owned(), plan, None, Some(cached));
    let error = match prepare_compilation(&resolved, PrepareOptions::without_mods(), None) {
        Ok(_) => panic!("strict preparation must not consume cached editor authority"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("recovery policy"), "must diagnose authority mismatch: {error}");

    let mut prepared = prepare_compilation(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                assembly_recovery: AssemblyRecoveryPolicy::EditorRetainRecovered,
                ..Default::default()
            },
            ..PrepareOptions::without_mods()
        },
        None,
    )
    .expect("matching editor-policy prepare");
    assert_eq!(prepared.assembly.recovery_policy, AssemblyRecoveryPolicy::EditorRetainRecovered);
    assert_eq!(prepared.syntax_assembly().recovery_policy, AssemblyRecoveryPolicy::EditorRetainRecovered);
    assert_eq!(
        prepared.typed.as_ref().expect("typed result").syntax_assembly().recovery_policy,
        AssemblyRecoveryPolicy::EditorRetainRecovered
    );
    prepared.typed = None;
    assert_eq!(
        prepared.syntax_assembly().recovery_policy,
        AssemblyRecoveryPolicy::EditorRetainRecovered,
        "untyped projection retains recovery authority too"
    );
    crate::services::entry_session::invalidate_project(directory.path());
}
