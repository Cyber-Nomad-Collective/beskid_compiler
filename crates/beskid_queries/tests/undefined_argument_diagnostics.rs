//! The real semantic gate must diagnose value arguments in reachable imported bodies.
use beskid_analysis::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};
use beskid_analysis::services::{PrepareOptions, ResolvedInput};
use beskid_queries::{
    BeskidDatabase, prepare_compilation_diagnostics_isolated, prepare_compilation_diagnostics_with_db,
};

#[test]
fn reachable_imported_undefined_argument_is_diagnosed_before_lowering() {
    for (declared, api) in [
        (false, "pub unit Accept(i64 value) {} pub unit Verify() { Accept(node); }"),
        (true, "pub unit Accept(i64 value) {} pub unit Verify() { i64 node = 7_i64; Accept(node); }"),
        (
            true,
            "pub unit Accept(i64 value) {} pub unit Forward(i64 node) { Accept(node); } pub unit Verify() { Forward(7_i64); }",
        ),
        (true, "const NODE = 7_i64; pub unit Accept(i64 value) {} pub unit Verify() { Accept(NODE); }"),
        (
            true,
            "pub unit Accept(i64 value) {} pub type Holder { pub i64 node, pub unit Send() { Accept(node); } } pub unit Verify() { Holder holder = Holder { node: 7_i64 }; holder.Send(); }",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let src = root.join("src");
        std::fs::create_dir(&src).unwrap();
        let main = "use Api; pub unit Main() { Api.Verify(); }";
        let path = src.join("Main.bd");
        std::fs::write(&path, main).unwrap();
        std::fs::write(src.join("Api.bd"), api).unwrap();
        let plan = CompilePlan {
            project_root: root.clone(),
            manifest_path: root.join("Fixture.bproj"),
            project_name: "Fixture".into(),
            source_root: src,
            target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_std_dependency: false,
        };
        let assembly =
            assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None)
                .unwrap();
        assert!(
            assembly.units.iter().any(|unit| unit.path.ends_with("Api.bd")),
            "actual import materializer must include Api"
        );
        let resolved = ResolvedInput {
            source_path: path,
            source: main.into(),
            compile_plan: Some(plan),
            prepared_workspace: None,
            workspace_summary: None,
            assembly: Some(assembly),
        };
        let mut db = BeskidDatabase::default();
        let (_, shared, _) =
            prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::default(), None).unwrap();
        let (_, isolated, _) =
            prepare_compilation_diagnostics_isolated(&resolved, PrepareOptions::default(), None).unwrap();
        for diagnostics in [shared, isolated] {
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == beskid_analysis::Severity::Error)
                .collect::<Vec<_>>();
            if declared {
                assert!(errors.is_empty(), "declared imported local must remain valid: {errors:?}");
            } else {
                let unknown =
                    errors.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1101")).collect::<Vec<_>>();
                assert_eq!(unknown.len(), 1, "undefined argument must be diagnosed once: {errors:?}");
                assert!(unknown[0].message.contains("node"), "diagnostic must identify the actual argument");
                assert!(unknown[0].src.name().ends_with("Api.bd"), "diagnostic belongs to imported source");
                assert_eq!(unknown[0].span.offset(), api.find("node").unwrap());
                assert_eq!(unknown[0].span.len(), "node".len());
            }
        }
    }
}
