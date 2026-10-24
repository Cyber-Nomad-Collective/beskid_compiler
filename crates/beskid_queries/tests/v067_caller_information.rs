//! Caller-information parameters (0.6.7) in checking: they are filled at a call that omits them,
//! an explicit argument is accepted, and they never count toward the required argument count.
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


/// Error diagnostics of `units[0]` prepared as the entry of an import-closure assembly.
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
    let (_, diagnostics, _) = prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::without_mods(), None)
        .expect("prepare diagnostics");
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| format!("{:?} {} @{:?}", diagnostic.code, diagnostic.message, diagnostic.span))
        .collect()
}

const ASSERT: (&str, &str) = (
    "Testing/Assert.bd",
    "pub unit Check(bool ok, string because, [CallerFilePath] string file, [CallerLineNumber] i64 line, [CallerColumnNumber] i64 column) { return; }",
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
fn a_missing_argument_reports_the_count_without_caller_parameters() {
    let found = errors(&[("Main.bd", "use Testing.Assert; pub unit Run() { Assert.Check(true); }"), ASSERT]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("expected 2, got 1"), "{found:#?}");
    assert!(!found[0].contains("expected 5"), "{found:#?}");
}
