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
#[test]
fn prepare_diagnostics_reports_nonconforming_inferred_generic_bound_with_code() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_generic_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } type Source: Reader { pub i64 Read() { return 1_i64; } } type NotSource { } i64 Consume<T>(T value) where T: Reader { return 0_i64; } i64 Forward<U>(U value) where U: Reader { return Consume(value); } unit Main() { Forward(Source {}); Consume(NotSource {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E1610")
                && diagnostic.message.contains("NotSource")
                && diagnostic.message.contains("Reader")
                && diagnostic.severity == Severity::Error
        }),
        "expected a structured E1610 diagnostic for the bounded call; got: {diagnostics:?}"
    );
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1610")).count(), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_reports_nonconforming_explicit_generic_bound_with_code() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_explicit_generic_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } type NotSource { } i64 Consume<T>(T value) where T: Reader { return 0_i64; } unit Main() { Consume<NotSource>(NotSource {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E1610")
                && diagnostic.message.contains("NotSource")
                && diagnostic.message.contains("Reader")
                && diagnostic.severity == Severity::Error
        }),
        "expected a structured E1610 diagnostic for the explicit type argument; got: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_unconstrained_generic_forwarding_with_code() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_unbounded_forward_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } type Source: Reader { pub i64 Read() { return 1_i64; } } i64 Consume<T>(T value) where T: Reader { return 0_i64; } i64 Forward<U>(U value) { return Consume(value); } unit Main() { Forward(Source {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E1610")
                && diagnostic.message.contains("U")
                && diagnostic.message.contains("Reader")
                && diagnostic.severity == Severity::Error
        }),
        "expected a structured E1610 diagnostic for unconstrained forwarding; got: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_reports_nested_generic_bound_failure_once() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_nested_generic_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } type NotSource { } i64 Consume<T>(T value) where T: Reader { return 0_i64; } unit Wrap<V>(V value) { return; } unit Main() { Wrap(Consume(NotSource {})); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    let bound_failures = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1610")).collect::<Vec<_>>();
    assert_eq!(bound_failures.len(), 1, "nested failing call should produce one structured E1610: {diagnostics:?}");
    assert!(bound_failures[0].message.contains("NotSource"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_enforces_qualified_contract_from_imported_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_imported_alias_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(root.join("Contracts.bd"), "pub contract Reader { i64 Read(); }").expect("contract source");
    let source = "use Contracts as Alias; type NotSource { } i64 Consume<T>(T value) where T: Alias.Reader { return 0_i64; } unit Main() { Consume(NotSource {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E1610")
                && diagnostic.message.contains("NotSource")
                && diagnostic.message.contains("Reader")
                && diagnostic.severity == Severity::Error
        }),
        "imported contract alias must enforce the bound; got: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_accepts_conforming_qualified_contract_from_imported_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_imported_module_bound_positive_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(root.join("Contracts.bd"), "pub contract Reader { i64 Read(); }").expect("contract source");
    let source = "use Contracts as Alias; type Source: Alias.Reader { pub i64 Read() { return 1_i64; } } i64 Consume<T>(T value) where T: Alias.Reader { return 0_i64; } unit Main() { Consume(Source {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "conforming qualified contract argument must satisfy the bound: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_direct_contract_item_import_alias() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_unsupported_item_import_alias_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(root.join("Contracts.bd"), "pub contract Reader { i64 Read(); }").expect("contract source");
    let source = "use Contracts.Reader as Alias; type NotSource { } i64 Consume<T>(T value) where T: Alias { return 0_i64; } unit Main() { Consume(NotSource {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1105") && diagnostic.severity == Severity::Error),
        "direct contract-item imports are not valid module imports: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_unresolved_bound_on_unused_function() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_unused_unknown_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "i64 Consume<T>(T value) where T: Missing { return 0_i64; } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.message.contains("Missing")
        }),
        "unresolved bound must fail before any call: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_invalid_bound_parameter_and_noncontract_target() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_invalid_bound_declarations_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } type NotContract { } i64 BadParameter<T>(T value) where U: Reader { return 0_i64; } i64 BadTarget<T>(T value) where T: NotContract { return 0_i64; } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    for name in ["U", "NotContract"] {
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error && diagnostic.message.contains(name)),
            "invalid bound declaration `{name}` must fail before any call: {diagnostics:?}"
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_resolves_same_named_bounds_within_sibling_inline_modules() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_sibling_inline_bounds_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "mod A { contract Reader { i64 Read(); } i64 Consume<T>(T value) where T: Reader { return 0_i64; } } mod B { contract Reader { i64 Read(); } } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "sibling contract must not make a lexical bound ambiguous: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_bound_visible_only_in_unrelated_inline_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_hidden_inline_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "mod Hidden { contract Reader { i64 Read(); } } i64 Consume<T>(T value) where T: Reader { return 0_i64; } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.message.contains("Reader")
        }),
        "unrelated inline contract must not satisfy an unqualified bound: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_keeps_duplicate_contract_error_within_one_inline_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_duplicate_inline_contract_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "mod A { contract Reader { i64 Read(); } contract Reader { i64 Read(); } } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1001")),
        "same lexical module must still reject duplicate contracts: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_resolves_bound_from_enclosing_inline_module_scope() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_parent_inline_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } mod A { i64 Consume<T>(T value) where T: Reader { return 0_i64; } } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "child module must inherit a visible parent contract: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_does_not_skip_noncontract_shadow_in_child_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_shadowed_inline_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } mod A { type Reader { } i64 Consume<T>(T value) where T: Reader { return 0_i64; } } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.message.contains("Reader")
        }),
        "child non-contract must shadow the parent bound target: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_preserves_imported_export_bound_owner() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_imported_export_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(
        root.join("Contracts.bd"),
        "pub contract Reader { i64 Read(); } pub type Source: Reader { pub i64 Read() { return 1_i64; } } pub i64 Consume<T>(T value) where T: Reader { return 0_i64; }",
    )
    .expect("api source");
    let source = "use Contracts as Alias; type NotSource { } unit Main() { Alias.Consume(Alias.Source {}); Alias.Consume(NotSource {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1610")),
        "imported bounded function must reject nonconforming caller: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1201")),
        "declared bound must resolve in Api, despite import into Main: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_preserves_imported_embedded_contract_owner() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_imported_embedded_owner_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(
        root.join("Contracts.bd"),
        "pub contract Reader { i64 Read(); } pub contract Advanced { Reader; } pub type Source: Advanced { pub i64 Read() { return 1_i64; } } pub i64 Consume<T>(T value) where T: Reader { return 0_i64; }",
    )
    .expect("api source");
    let source = "use Contracts as Alias; unit Main() { Alias.Consume(Alias.Source {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "imported embedded contract must preserve its declaring scope and conformance: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_rejects_private_qualified_contract_bound_from_other_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_private_qualified_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    std::fs::write(root.join("Contracts.bd"), "contract PrivateReader { i64 Read(); }")
        .expect("contract source");
    let source = "use Contracts as Alias; i64 Consume<T>(T value) where T: Alias.PrivateReader { return 0_i64; } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E1201")
                && diagnostic.message.contains("PrivateReader")
        }),
        "private external contract must not satisfy an unused declaration's bound: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_accepts_private_qualified_contract_in_declaring_module() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_local_private_qualified_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "mod A { contract Reader { i64 Read(); } i64 Consume<T>(T value) where T: A.Reader { return 0_i64; } } unit Main() { return; }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "same lexical module may use its private contract via a qualified path: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prepare_diagnostics_accepts_bound_via_embedded_contract() {
    let test_id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("beskid_prepare_embedded_contract_bound_{test_id}"));
    std::fs::create_dir_all(&root).expect("test source root");
    let entry_path = root.join("Main.bd");
    let source = "contract Reader { i64 Read(); } contract Advanced { Reader; } type Source: Advanced { pub i64 Read() { return 1_i64; } } i64 Consume<T>(T value) where T: Reader { return 0_i64; } unit Main() { Consume(Source {}); }";
    std::fs::write(&entry_path, source).expect("entry source");

    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let (_, diagnostics, _) = prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        None,
    )
    .expect("prepare diagnostics");
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
        "embedding Advanced -> Reader must satisfy Reader bound: {diagnostics:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}
