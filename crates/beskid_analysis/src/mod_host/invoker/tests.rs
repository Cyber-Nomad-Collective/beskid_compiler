use super::super::context::ModInvocationContext;
use super::super::types::ContractRegistration;
use super::*;

fn r(contract: &str, ty: &str, sym: &str) -> ContractRegistration {
    ContractRegistration { contract_id: contract.to_owned(), type_id: ty.to_owned(), entry_symbol: sym.to_owned() }
}

fn empty_collect_request() -> ModInvocationContext {
    ModInvocationContext::empty()
}

#[test]
fn stub_records_each_invocation_kind() {
    let invoker = StubContractInvoker::new();
    let mut context = empty_collect_request();
    invoker.invoke_collector(&r("Beskid.Compiler.Collect.Collector", "T1", "c"), &context.collect_request).unwrap();
    invoker
        .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"), &context.generation_request(&[]))
        .unwrap();
    invoker
        .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "T3", "a"), &context.collect_request, None)
        .unwrap();
    invoker.invoke_rewriter(&r("Beskid.Compiler.Collect.Rewriter", "T4", "r"), &context.collect_request).unwrap();
    let log = invoker.invocations();
    assert_eq!(log.len(), 4);
    assert!(matches!(log[0], InvocationKind::Collector { .. }));
    assert!(matches!(log[1], InvocationKind::Generator { .. }));
    assert!(matches!(log[2], InvocationKind::Analyzer { .. }));
    assert!(matches!(log[3], InvocationKind::Rewriter { .. }));
}

#[test]
fn scripted_overlays_generator_contributions() {
    let invoker = ScriptedContractInvoker::new()
        .with_generator_contribution("T2", vec!["pub fn synthetic_generated() { return; }".into()]);
    let mut context = empty_collect_request();
    let outcome = invoker
        .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"), &context.generation_request(&[]))
        .unwrap();
    assert_eq!(outcome.typed_items.len(), 1);
}

#[test]
fn scripted_overlays_analyzer_diagnostics() {
    let invoker = ScriptedContractInvoker::new().with_analyzer_diagnostic(
        "TA",
        vec![AnalyzerDiagnostic {
            code: "ModA0001".into(),
            message: "test".into(),
            severity: AnalyzerSeverity::Warning,
            span: Some((4, 8)),
        }],
    );
    let context = empty_collect_request();
    let outcome = invoker
        .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "TA", "a"), &context.collect_request, None)
        .unwrap();
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].code, "ModA0001");
    assert_eq!(outcome.diagnostics[0].span, Some((4, 8)));
}

/// A mod returns a diagnostic + a `QuickFix`; the fix round-trips through `AnalyzerFix`
/// (scripted overlay populates `outcome.fixes`) and maps to a `SyntaxFix` via
/// `analyzer_fix_to_syntax_fix`. The `diagnostic_index` links the fix to its diagnostic.
#[test]
fn scripted_overlays_analyzer_fixes_and_maps_to_syntax_fix() {
    use super::super::diagnostics::{SyntaxTextEditKind, analyzer_fix_to_syntax_fix};

    let diagnostic = AnalyzerDiagnostic {
        code: "ModA0001".into(),
        message: "unused import".into(),
        severity: AnalyzerSeverity::Warning,
        span: Some((0, 4)),
    };
    let fix = AnalyzerFix {
        diagnostic_index: 0,
        title: "Remove import".into(),
        edits: vec![
            RewriteEdit::Delete { start: 0, end: 4 },
            RewriteEdit::Insert { offset: 0, text: "// ".into() },
        ],
    };
    let invoker = ScriptedContractInvoker::new()
        .with_analyzer_diagnostic("TA", vec![diagnostic.clone()])
        .with_analyzer_fix("TA", vec![fix.clone()]);
    let context = empty_collect_request();
    let outcome = invoker
        .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "TA", "a"), &context.collect_request, None)
        .unwrap();

    // Round-trip through AnalyzerFix: the scripted overlay populates outcome.fixes.
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.fixes.len(), 1);
    assert_eq!(outcome.fixes[0].diagnostic_index, 0);
    assert_eq!(outcome.fixes[0].title, "Remove import");
    assert_eq!(outcome.fixes[0].edits.len(), 2);

    // Map to SyntaxFix: the fix resolves the linked diagnostic and tags the source.
    let source = format!("beskid:mod:{}", outcome.type_id);
    let syntax_fix =
        analyzer_fix_to_syntax_fix(&outcome.fixes[0], &outcome, &source).expect("fix maps to SyntaxFix");
    assert_eq!(syntax_fix.source, "beskid:mod:TA");
    assert_eq!(syntax_fix.diagnostic_code, "ModA0001");
    assert_eq!(syntax_fix.title, "Remove import");
    assert_eq!(syntax_fix.edits.len(), 2);
    assert_eq!(syntax_fix.edits[0].kind, SyntaxTextEditKind::Delete);
    assert_eq!(syntax_fix.edits[1].kind, SyntaxTextEditKind::Insert);
    assert_eq!(syntax_fix.edits[1].text, "// ");
}

/// A fix whose `diagnostic_index` is out of range is dropped by `analyzer_fix_to_syntax_fix`
/// (fail-closed — mirrors `unmarshal_fixes` in `native.rs`).
#[test]
fn analyzer_fix_with_out_of_range_index_drops_when_mapping_to_syntax_fix() {
    use super::super::diagnostics::analyzer_fix_to_syntax_fix;

    let outcome = AnalyzerOutcome {
        type_id: "TA".into(),
        diagnostics: vec![AnalyzerDiagnostic {
            code: "ModA0001".into(),
            message: "only one".into(),
            severity: AnalyzerSeverity::Warning,
            span: Some((0, 4)),
        }],
        fixes: vec![AnalyzerFix {
            diagnostic_index: 5, // out of range (only 1 diagnostic)
            title: "stale".into(),
            edits: Vec::new(),
        }],
    };
    let source = format!("beskid:mod:{}", outcome.type_id);
    assert!(analyzer_fix_to_syntax_fix(&outcome.fixes[0], &outcome, &source).is_none());
}

#[test]
fn scripted_overlays_rewriter_edits() {
    let invoker = ScriptedContractInvoker::new().with_rewriter_edits(
        "TR",
        vec![
            RewriteEdit::Replace { start: 0, end: 4, text: "UNIT".into() },
            RewriteEdit::Insert { offset: 12, text: " // touched".into() },
        ],
    );
    let context = empty_collect_request();
    let outcome = invoker
        .invoke_rewriter(&r("Beskid.Compiler.Collect.Rewriter", "TR", "r"), &context.collect_request)
        .unwrap();
    assert_eq!(outcome.edits.len(), 2);
    assert!(matches!(outcome.edits[0], RewriteEdit::Replace { .. }));
    assert!(matches!(outcome.edits[1], RewriteEdit::Insert { .. }));
}

#[test]
fn analyzer_diagnostic_defaults_to_warning_severity_and_no_span() {
    let diag = AnalyzerDiagnostic::default();
    assert_eq!(diag.severity, AnalyzerSeverity::Warning);
    assert_eq!(diag.span, None);
    assert!(diag.code.is_empty());
}
