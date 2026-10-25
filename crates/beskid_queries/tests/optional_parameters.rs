//! Optional parameters in checking: a call may omit trailing optional arguments (defaults and
//! caller information are filled once the callee is known, for functions, methods and contract
//! methods), an explicit argument is accepted, arity errors
//! name the required-to-total range, and invalid declarations are reported at the declaration.
use std::sync::Arc;

use beskid_analysis::Severity;
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::{
    PrepareOptions, ResolvedInput, parse_program_with_source_name, synthetic_compile_plan_for_source,
};
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;
use beskid_queries::{BeskidDatabase, prepare_compilation_diagnostics_with_db};

/// Error diagnostics of `units[0]` prepared as the entry of an import-closure assembly, as
/// `<code> <message> @<span>`.
fn errors(units: &[(&str, &str)]) -> Vec<String> {
    let directory = tempfile::tempdir().unwrap();
    let generation = SyntaxGenerationId(611);
    let units = units
        .iter()
        .map(|(relative, source)| {
            let path = directory.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, source).unwrap();
            SourceUnit {
                logical_name: (*relative).into(),
                origin_path: path.clone(),
                program: parse_program_with_source_name(path.to_str().unwrap(), source).unwrap(),
                path,
                source: (*source).into(),
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
        source: units[0].source.clone(),
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
        prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::without_mods(), None)
            .expect("prepare diagnostics");
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| format!("{:?} {} @{:?}", diagnostic.code, diagnostic.message, diagnostic.span))
        .collect()
}

const ASSERT: (&str, &str) = (
    "Testing/Assert.bd",
    "pub unit Check(bool ok, string because = \"\", [CallerFilePath] string file = \"\", [CallerLineNumber] i64 line = 0, [CallerColumnNumber] i64 column = 0) { return; }\n\
     pub unit Scale(f64 factor, i64 times = 2, u8 small = 7, bool strict = true) { return; }",
);

#[test]
fn a_call_that_omits_the_caller_parameters_checks() {
    let found = errors(&[("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Check(true, \"m\"); }"), ASSERT]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_call_with_explicit_caller_arguments_checks() {
    let found = errors(&[
        ("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Check(true, \"m\", \"x.bd\", 3_i64, 4_i64); }"),
        ASSERT,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_call_that_omits_because_and_the_caller_parameters_checks() {
    let found = errors(&[("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Check(true); }"), ASSERT]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn omitted_defaults_take_their_parameter_types() {
    let found = errors(&[
        (
            "Main.bd",
            "use Testing.Assert; pub unit Run() { Assert.Scale(1.5); Assert.Scale(1.5, 3); Assert.Scale(1.5, 3, 9, false); }",
        ),
        ASSERT,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_missing_required_argument_names_the_range() {
    let found = errors(&[("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Check(); }"), ASSERT]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("E1204") && found[0].contains("expected 1\u{2013}5 arguments, got 0"), "{found:#?}");
}

#[test]
fn too_many_arguments_name_the_range() {
    let found =
        errors(&[("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Scale(1.5, 1, 2, true, 4); }"), ASSERT]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("expected 1\u{2013}4 arguments, got 5"), "{found:#?}");
}

#[test]
fn a_function_without_optional_parameters_names_one_count() {
    let found = errors(&[("Main.bd", "pub unit Two(i64 a, i64 b) { return; } pub unit Run() { Two(1); }")]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("expected 2 arguments, got 1"), "{found:#?}");
}

#[test]
fn an_invalid_default_is_reported_at_the_declaration() {
    let found = errors(&[("Main.bd", "pub unit Take(i64 count = \"x\") { return; } pub unit Run() { Take(1); }")]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("E1253")
            && found[0].contains("default value `\"x\"` does not fit parameter `count` of type `i64`"),
        "{found:#?}"
    );
}

const STEPPER: &str = "pub contract Stepper { i64 Step(i64 base, i64 by = 1); }\n";

#[test]
fn a_method_call_omits_the_method_defaults() {
    let found = errors(&[(
        "Main.bd",
        "pub type Counter { pub i64 Add(i64 base, i64 step = 1) { return base + step; } }\n\
         pub i64 Run() { Counter counter = Counter {}; return counter.Add(2) + counter.Add(2, 3); }",
    )]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_method_call_without_its_required_arguments_names_the_range_with_the_receiver() {
    let found = errors(&[(
        "Main.bd",
        "pub type Counter { pub i64 Add(i64 base, i64 step = 1) { return base + step; } }\n\
         pub i64 Run() { Counter counter = Counter {}; return counter.Add(); }",
    )]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("expected 2\u{2013}3 arguments, got 1"), "{found:#?}");
}

#[test]
fn an_implementation_inherits_the_contract_defaults() {
    let source = format!(
        "{STEPPER}pub type Walker : Stepper {{ pub i64 Step(i64 base, i64 by) {{ return base + by; }} }}\n\
         pub i64 Run() {{ Walker walker = Walker {{}}; return walker.Step(1); }}"
    );
    let found = errors(&[("Main.bd", source.as_str())]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_call_through_a_contract_bound_gets_the_contract_defaults() {
    let source = format!(
        "{STEPPER}pub type Walker : Stepper {{ pub i64 Step(i64 base, i64 by) {{ return base + by; }} }}\n\
         i64 Use<T>(T stepper) where T: Stepper {{ return stepper.Step(5); }}\n\
         pub i64 Run() {{ Walker walker = Walker {{}}; return Use<Walker>(walker); }}"
    );
    let found = errors(&[("Main.bd", source.as_str())]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn an_implementation_that_repeats_a_different_default_is_e1612() {
    let source =
        format!("{STEPPER}pub type Walker : Stepper {{ pub i64 Step(i64 base, i64 by = 2) {{ return base + by; }} }}");
    let found = errors(&[("Main.bd", source.as_str())]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("E1612")
            && found[0].contains("`Step` gives parameter `by` the default `2`, but the contract declares `1`"),
        "{found:#?}"
    );
}

#[test]
fn an_implementation_that_repeats_the_same_default_checks() {
    let source =
        format!("{STEPPER}pub type Walker : Stepper {{ pub i64 Step(i64 base, i64 by = 1) {{ return base + by; }} }}");
    let found = errors(&[("Main.bd", source.as_str())]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_function_in_an_inline_module_takes_defaults() {
    let found = errors(&[(
        "Main.bd",
        "mod Inner { pub i64 Go(i64 x = 1) { return x; } }\npub i64 Run() { return Inner.Go() + Inner.Go(2); }",
    )]);
    assert!(found.is_empty(), "{found:#?}");
}
