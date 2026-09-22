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
