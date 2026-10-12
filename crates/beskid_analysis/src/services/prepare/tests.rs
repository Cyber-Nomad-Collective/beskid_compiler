use super::collect_analyzer_diagnostics;
use crate::analysis::diagnostics::Severity;
use crate::mod_host::{
    AnalyzerDiagnostic, AnalyzerSeverity, ContractInvoker, ContractRegistration, ModHostAnalyzeResult,
    ModInvocationContext, ScriptedContractInvoker,
};
use crate::projects::SourceUnit;
use crate::services::semantic::require_no_semantic_errors;
use crate::services::parse_program_with_source_name;

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
