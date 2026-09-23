use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::{
    PrepareOptions, ResolvedInput, parse_program_with_source_name, synthetic_compile_plan_for_source,
};
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;
use beskid_queries::{
    BeskidDatabase, prepare_compilation_diagnostics_isolated, prepare_compilation_diagnostics_with_db,
};

#[test]
fn imported_try_diagnostics_share_exact_result_error_authority() {
    for (error, expected_errors) in [("NetworkError", 0), ("OtherError", 1)] {
        let directory = tempfile::tempdir().unwrap();
        let main = format!(
            "use Core.Results; use Network.Api; Result<unit, {error}> Main() {{ i64 value = Api.Accept()?; return Result::Ok(()); }}"
        );
        let sources = [
            ("Main.bd", main.as_str()),
            (
                "Network/Api.bd",
                "use Core.Results; pub enum NetworkError { Closed() } pub enum OtherError { Closed() } pub Result<i64, NetworkError> Accept() { return Result::Ok(1_i64); }",
            ),
            ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ];
        let generation = SyntaxGenerationId(158);
        let units = sources
            .into_iter()
            .map(|(relative, source)| {
                let path = directory.path().join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, source).unwrap();
                SourceUnit {
                    logical_name: relative.into(),
                    origin_path: path.clone(),
                    program: parse_program_with_source_name(path.to_str().unwrap(), source).unwrap(),
                    path,
                    source: source.into(),
                }
            })
            .collect::<Vec<_>>();
        let plan = synthetic_compile_plan_for_source(&units[0].path);
        let roots = EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.path().into() },
            dependencies: vec![],
        };
        let indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
        let index = Arc::new(ModuleIndex::build(&units, &indexes, &roots, &plan));
        let resolved = ResolvedInput {
            source_path: units[0].path.clone(),
            source: main,
            compile_plan: Some(plan),
            prepared_workspace: None,
            workspace_summary: None,
            assembly: Some(ProgramAssembly::new(
                roots,
                Arc::new(units),
                0,
                AssemblyDiscovery::ImportClosure,
                index,
                false,
                generation,
            )),
        };
        let mut db = BeskidDatabase::default();
        let (_, diagnostics, _) =
            prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::default(), None)
                .expect("ordinary preparation preserves truthful try diagnostics");
        let errors = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1222")).count();
        assert_eq!(errors, expected_errors, "{diagnostics:?}");
        if expected_errors == 0 {
            assert!(
                !diagnostics.iter().any(|diagnostic| diagnostic.severity == beskid_analysis::Severity::Error),
                "{diagnostics:?}"
            );
        }
        let (_, isolated, _) = prepare_compilation_diagnostics_isolated(&resolved, PrepareOptions::default(), None)
            .expect("owned editor jobs use the same try authority");
        assert_eq!(
            isolated.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1222")).count(),
            expected_errors,
            "{isolated:?}"
        );
    }
}

/// Build a single-unit, no-Std assembly for `source` and return the E1222 counts from the
/// query-backed try authority (shared and isolated) and from the analysis-only path, whose
/// staged try rule uses the pre-normalize precheck instead of a query authority.
fn local_try_diagnostic_counts(source: &str) -> (usize, usize, usize) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let generation = SyntaxGenerationId(211);
    let units = vec![SourceUnit {
        logical_name: "Main.bd".into(),
        origin_path: path.clone(),
        program: parse_program_with_source_name(path.to_str().unwrap(), source).unwrap(),
        path: path.clone(),
        source: source.into(),
    }];
    let plan = synthetic_compile_plan_for_source(&path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: directory.path().into() },
        dependencies: vec![],
    };
    let indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
    let index = Arc::new(ModuleIndex::build(&units, &indexes, &roots, &plan));
    let resolved = ResolvedInput {
        source_path: path,
        source: source.into(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: Some(ProgramAssembly::new(
            roots,
            Arc::new(units),
            0,
            AssemblyDiscovery::ImportClosure,
            index,
            false,
            generation,
        )),
    };
    let count = |diagnostics: &[beskid_analysis::SemanticDiagnostic]| {
        diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1222")).count()
    };
    let mut db = BeskidDatabase::default();
    let (_, shared, _) = prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::default(), None)
        .expect("shared try authority");
    let (_, isolated, _) = prepare_compilation_diagnostics_isolated(&resolved, PrepareOptions::default(), None)
        .expect("isolated try authority");
    let (_, precheck, _) =
        beskid_analysis::services::prepare_compilation_diagnostics(&resolved, PrepareOptions::default(), None)
            .expect("analysis-only precheck");
    (count(&shared), count(&isolated), count(&precheck))
}

const LOCAL_GENERIC_RESULT: &str = "enum Error { Failed() }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
";

#[test]
fn try_accepts_let_bound_local_generic_result() {
    let source = format!(
        "{LOCAL_GENERIC_RESULT}Result<i32, Error> Propagate(Result<i32, Error> input) {{
    Result<i32, Error> bound = input;
    i32 value = bound?;
    return Result::Ok(value);
}}
"
    );
    assert_eq!(local_try_diagnostic_counts(&source), (0, 0, 0));
}

#[test]
fn try_accepts_let_bound_result_after_statement_match() {
    let source = format!(
        "{LOCAL_GENERIC_RESULT}Result<i32, Error> Propagate(Result<i32, Error> input) {{
    Result<i32, Error> bound = input;
    match input {{
        Result::Ok(_) => {{}},
        Result::Error(_) => {{}},
    }};
    i32 value = bound?;
    return Result::Ok(value);
}}
"
    );
    assert_eq!(local_try_diagnostic_counts(&source), (0, 0, 0));
}

#[test]
fn try_rejects_non_result_operands() {
    for operand in ["i32 bound = 1;", "Choice bound = Choice::Ok(1);"] {
        let source = format!(
            "{LOCAL_GENERIC_RESULT}enum Choice {{ Ok(i32 value), Other(i32 other) }}
Result<i32, Error> Propagate() {{
    {operand}
    i32 value = bound?;
    return Result::Ok(value);
}}
"
        );
        let (shared, isolated, precheck) = local_try_diagnostic_counts(&source);
        assert!(shared >= 1 && isolated >= 1 && precheck >= 1, "{operand}: {shared} {isolated} {precheck}");
    }
}

/// Item 1 (spec conformance): an enum that declares only `Ok` (no `Error` variant) is not
/// Result-shaped per OpenSpec `language-meta--contracts-and-effects--error-handling`, "Postfix
/// try operator". The full type check (the lower-spine `type_try_expression`) must reject it
/// exactly like the precheck stage does, not just the precheck.
///
/// Item 2 (no duplicate diagnostic): the try-authority path and the lower-spine full type check
/// both judge this operand invalid using the same shared predicate, so without deduping they
/// would each contribute one E1222 for the same span. Assert exactly one survives.
#[test]
fn try_rejects_ok_only_enum_with_exactly_one_diagnostic() {
    let source = format!(
        "{LOCAL_GENERIC_RESULT}enum Choice {{ Ok(i32 value), Other(i32 other) }}
Result<i32, Error> Propagate() {{
    Choice bound = Choice::Ok(1);
    i32 value = bound?;
    return Result::Ok(value);
}}
"
    );
    let (shared, isolated, precheck) = local_try_diagnostic_counts(&source);
    assert_eq!(shared, 1, "try authority + full type check must emit exactly one E1222, got {shared}");
    assert_eq!(isolated, 1, "isolated authority + full type check must emit exactly one E1222, got {isolated}");
    assert_eq!(precheck, 1, "analysis-only precheck must emit exactly one E1222, got {precheck}");
}

/// Item 3: the early stage-7 try walker must descend into `if`/`while`/`for` bodies so an
/// invalid `?` nested inside one is reported by the precheck stage, not only by the later
/// full type check.
#[test]
fn try_precheck_descends_into_nested_control_flow_bodies() {
    for wrapper in [
        "if (true) { i32 value = bound?; }",
        "while (true) { i32 value = bound?; break; }",
        "for item in items { i32 value = bound?; }",
    ] {
        let source = format!(
            "{LOCAL_GENERIC_RESULT}Result<unit, Error> Propagate(i32[] items) {{
    i32 bound = 1;
    {wrapper}
    return Result::Ok(());
}}
"
        );
        let (_, _, precheck) = local_try_diagnostic_counts(&source);
        assert!(precheck >= 1, "{wrapper}: precheck did not descend into the nested body ({precheck})");
    }
}

/// Item 5: `i32(flag)` where `flag: bool` is not a valid primitive numeric conversion argument
/// (the ISLE lowering contract, "Explicit primitive numeric conversion lowering", limits
/// conversions to exactly one primitive numeric argument). This must be a clear, typed E1228
/// diagnostic from the type checker, not an opaque ISLE `MissingRuleOrFact` surfaced later.
#[test]
fn primitive_conversion_rejects_non_numeric_argument() {
    let source = "i32 Convert(bool flag) { return i32(flag); }";
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let generation = SyntaxGenerationId(311);
    let units = vec![SourceUnit {
        logical_name: "Main.bd".into(),
        origin_path: path.clone(),
        program: parse_program_with_source_name(path.to_str().unwrap(), source).unwrap(),
        path: path.clone(),
        source: source.into(),
    }];
    let plan = synthetic_compile_plan_for_source(&path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: directory.path().into() },
        dependencies: vec![],
    };
    let indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
    let index = Arc::new(ModuleIndex::build(&units, &indexes, &roots, &plan));
    let resolved = ResolvedInput {
        source_path: path,
        source: source.into(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: Some(ProgramAssembly::new(
            roots,
            Arc::new(units),
            0,
            AssemblyDiscovery::ImportClosure,
            index,
            false,
            generation,
        )),
    };
    let mut db = BeskidDatabase::default();
    let (_, diagnostics, _) =
        prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::default(), None)
            .expect("diagnostics collection must not hard-fail on a typed conversion error");
    let e1228 = diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1228")).count();
    assert_eq!(e1228, 1, "expected exactly one E1228 for a non-numeric conversion argument: {diagnostics:?}");
}
