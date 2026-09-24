use std::sync::atomic::{AtomicU64, Ordering};

use super::{
    PrepareOptions, collect_analyzer_diagnostics, prepare_compilation, prepare_compilation_diagnostics,
    prepare_compilation_diagnostics_isolated,
};
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
            ..Default::default()
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

#[test]
fn prepare_diagnostics_collect_without_legacy_document_snapshot() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_diags_no_snapshot_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    // Intentional unresolved name so prepare-spine emits a diagnostic.
    let source = "i32 Main() { return Missing; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (prepared, diags, _fixes) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");

    let syntax = prepared.syntax_assembly();
    let expected = entry_path.canonicalize().unwrap_or(entry_path.clone());
    let actual = syntax.entry_unit().path.canonicalize().unwrap_or_else(|_| syntax.entry_unit().path.clone());
    assert_eq!(actual, expected);
    assert!(!diags.is_empty(), "prepare-spine diagnostics must surface without DocumentAnalysisSnapshot");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn isolated_diagnostics_do_not_read_or_replace_entry_session_assembly() {
    crate::services::invalidate_entry_sessions();
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_isolated_diags_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let first_source = "i32 Main() { return 1; }";
    std::fs::write(&entry_path, first_source).expect("entry source");
    let plan = synthetic_compile_plan_for_source(&entry_path);
    let first = resolved_input_from_plan(entry_path.clone(), first_source.to_string(), plan.clone(), None, None);
    prepare_compilation(&first, PrepareOptions::default(), None).expect("seed cached prepare");

    let second_source = "i32 Main() { return MissingNew; }";
    let assembly_options =
        crate::projects::assembly_options_for_prepare(&plan, FrontEndOptions::default().assembly_discovery);
    let second_assembly =
        crate::projects::assemble_program(&plan, None, &entry_path, Some(second_source), &assembly_options, None)
            .expect("second assembly");
    let second = resolved_input_from_plan(
        entry_path.clone(),
        second_source.to_string(),
        plan.clone(),
        None,
        Some(second_assembly),
    );
    let (prepared, diagnostics, _) = prepare_compilation_diagnostics_isolated(
        &second,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("isolated diagnostics");

    assert_eq!(prepared.assembly.entry_unit().source, second_source);
    assert!(!diagnostics.is_empty(), "second assembly should diagnose its unresolved name");
    let fingerprint = crate::services::SessionFingerprint::for_entry(&plan, &entry_path);
    let cached = crate::services::entry_session::cached_compilation_session(&fingerprint)
        .expect("seeded entry session remains cached");
    assert_eq!(cached.assembly.entry_unit().source, first_source);

    crate::services::invalidate_entry_sessions();
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
    let outcome =
        invoker.invoke_analyzer(&registration, &context.collect_request, None).expect("scripted analyzer invocation");
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
